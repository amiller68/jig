//! Worker — the single abstraction for a Claude Code session.
//!
//! A Worker owns its identity and a [`WorktreeRef`] pointing at its
//! git worktree on disk.  The full [`Worktree`] (wrapping a git2 repo
//! handle) is resolved on demand — we never serialize what we can derive.

pub mod checks;
pub mod events;
mod status;

pub use checks::{PrChecks, PrReport, PrStatus};
pub use status::{MuxStatus, WorkerStatus};

use std::path::Path;

use uuid::Uuid;

use url::Url;

use events::{Event, EventKind, PrHealth, TerminalKind, WorkerState};
use jig_core::agents::Agent;
use jig_core::error::Result;
use jig_core::git::{Branch, Repo, Worktree, WorktreeRef};
use jig_core::github::{GitHub, GitHubClient, PrState};

use checks::{check_ci, check_commits, check_conflicts, check_reviews};
use jig_core::issues::issue::IssueRef;
use jig_core::mux::Mux;
use jig_core::prompt::Prompt;

use crate::context::{self, Config, RepoEntry};

const SPAWN_PREAMBLE: &str = r#"AUTONOMOUS MODE: You have been spawned by jig as a parallel worker in auto mode (--dangerously-skip-permissions). Work independently without human interaction.

YOUR GOAL: Complete the task below and create a draft PR. Definition of done: code committed (conventional commits), draft PR created via `jig pr` or /draft, and issue marked complete (see completion instructions in the task). Call /review when ready.

IMPORTANT: Create the draft PR using `jig pr` (or `/draft`, which wraps it). NEVER use `gh pr create` directly — it bypasses parent branch resolution and will target the wrong base branch.

HOW MONITORING WORKS: A daemon watches your activity via tool-use events. If you go idle or get stuck for ~5 minutes, you'll receive automated nudge messages (up to {{max_nudges}}). After that, a human is notified. Do not wait for input.

IF YOU GET STUCK:
- Do NOT enter plan mode or ask for confirmation — just proceed
- If a command fails, try to fix it yourself
- If tests fail, debug and fix them
- If unsure about an approach, pick the simplest one and go
- If truly blocked, explain what's blocking you so the nudge system can relay it

TASK:
{{task_context}}
"#;

/// A Worker is a Claude Code session in an isolated git worktree.
///
/// Everything is derived at runtime via the [`Worktree`] handle.
#[derive(Debug, Clone)]
pub struct Worker {
    pub(crate) id: Uuid,
    pub(crate) branch: Branch,
    pub(crate) path: WorktreeRef,
    pub(crate) issue_ref: Option<IssueRef>,
}

impl From<&Worktree> for Worker {
    fn from(wt: &Worktree) -> Self {
        Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref: None,

        }
    }
}

impl Worker {
    pub fn from_branch(repo_root: &Path, branch: Branch) -> Self {
        let worktree_path = repo_root.join(crate::context::JIG_DIR).join(&*branch);
        Self {
            id: Uuid::nil(),
            branch,
            path: WorktreeRef::new(worktree_path),
            issue_ref: None,

        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn branch(&self) -> &Branch {
        &self.branch
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn issue_ref(&self) -> Option<&IssueRef> {
        self.issue_ref.as_ref()
    }

    pub fn worker_key(&self) -> String {
        format!("{}/{}", self.repo_name(), self.branch)
    }

    pub fn worktree(&self) -> Result<Worktree> {
        Ok(self.path.open()?)
    }

    pub fn event_log(&self) -> Result<events::EventLog> {
        let repo_name = self.repo_name();
        let log = events::event_log_for_worker(&repo_name, &self.branch)?;
        Ok(log)
    }

    pub fn worker_status(&self) -> Option<WorkerStatus> {
        let log = self.event_log().ok()?;
        if !log.exists() {
            return None;
        }
        let config = crate::context::Config::load().unwrap_or_default();
        let mut state: WorkerState = log.reduce().ok()?;
        state.check_silence(&config);
        Some(state.status)
    }

    pub fn fail_reason(&self) -> Option<String> {
        let log = self.event_log().ok()?;
        let events = log.read_all().ok()?;
        events.iter().rev().find_map(|e| {
            if let EventKind::Terminal {
                reason: Some(r), ..
            } = &e.kind
            {
                Some(r.clone())
            } else {
                None
            }
        })
    }

    pub fn remove(&self, force: bool) -> Result<()> {
        Ok(self.worktree()?.remove(force)?)
    }

    pub fn unregister(&self) -> Result<()> {
        if let Ok(log) = self.event_log() {
            let _ = log.remove();
        }
        Ok(())
    }

    pub fn discover(repo: &Repo) -> Vec<Self> {
        let mut workers: Vec<Self> = repo
            .list_worktrees()
            .unwrap_or_default()
            .iter()
            .map(Self::from)
            .collect();
        workers.sort_by(|a, b| a.branch.cmp(&b.branch));
        workers
    }

    pub fn repo_name(&self) -> String {
        self.path
            .parent()
            .and_then(|jig_dir| jig_dir.parent())
            .and_then(|root| root.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    // ── Mux operations ─────────────────────────────────────────────

    pub fn has_mux_window(&self, mux: &dyn Mux) -> bool {
        mux.window_exists(&self.branch)
    }

    pub fn is_agent_running(&self, mux: &dyn Mux) -> bool {
        mux.is_running(&self.branch)
    }

    pub fn mux_status(&self, mux: &dyn Mux) -> MuxStatus {
        if !mux.window_exists(&self.branch) {
            MuxStatus::NotFound
        } else if mux.is_running(&self.branch) {
            MuxStatus::Running
        } else {
            MuxStatus::Exited
        }
    }

    pub fn spawn(
        repo: &Repo,
        branch: &Branch,
        base: &Branch,
        agent: &Agent,
        task: Prompt,
        auto: bool,
        issue_ref: Option<IssueRef>,
        copy_files: &[std::path::PathBuf],
        on_create: Option<std::process::Command>,
        mux: &dyn Mux,
    ) -> Result<Self> {
        let repo_root = repo.clone_path();
        let branch_name = branch.to_string();

        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let event_log = events::event_log_for_worker(&repo_name, &branch_name)?;
        event_log.reset()?;

        let _ = event_log.append(&Event::now(EventKind::Initializing {
            branch: branch_name.clone(),
            base: base.to_string(),
            auto,
        }));

        let wt = match Worktree::create(repo, branch, base, copy_files, on_create) {
            Ok(wt) => wt,
            Err(e) => {
                let _ = event_log.append(&Event::now(EventKind::Terminal {
                    terminal: TerminalKind::Failed,
                    reason: Some(e.to_string()),
                }));
                return Err(e.into());
            }
        };

        let issue = issue_ref
            .clone()
            .unwrap_or_else(|| IssueRef::new(branch_name.clone()));
        let _ = event_log.append(&Event::now(EventKind::Spawn {
            branch: branch_name,
            repo: repo_name,
            issue,
        }));

        let worker = Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref,
        };

        let task_context = task.render()?;
        let context = Prompt::new(SPAWN_PREAMBLE)
            .var("task_context", &task_context);
        mux.create_window(&worker.branch, &worker.path)?;
        let cmd = agent.spawn(context)?;
        mux.send_keys(&worker.branch, &[&cmd, "Enter"])?;

        Ok(worker)
    }

    pub fn resume(wt: &Worktree, agent: &Agent, task_context: &str, mux: &dyn Mux) -> Result<Self> {
        let worker = Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref: None,
        };

        let context = Prompt::new(SPAWN_PREAMBLE)
            .var("task_context", task_context);

        if let Ok(event_log) = worker.event_log() {
            let _ = event_log.append(&Event::now(EventKind::Resume));
        }

        mux.create_window(&worker.branch, &worker.path)?;
        let cmd = agent.resume(context)?;
        mux.send_keys(&worker.branch, &[&cmd, "Enter"])?;

        Ok(worker)
    }

    pub fn nudge(&self, prompt: Prompt, mux: &dyn Mux) -> Result<()> {
        let nudge_type_key = prompt.name().to_string();
        let message = prompt.render()?;

        mux.send_message(&self.branch, &message)?;

        if let Ok(event_log) = self.event_log() {
            let _ = event_log.append(&Event::now(EventKind::Nudge {
                nudge_type: nudge_type_key,
                message: message.clone(),
            }));
        }

        Ok(())
    }

    pub fn kill(&self, mux: &dyn Mux) -> Result<()> {
        mux.kill_window(&self.branch)?;
        Ok(())
    }

    pub fn attach(&self, mux: &dyn Mux) -> Result<()> {
        if !mux.window_exists(&self.branch) {
            if self.path.exists() {
                if let Some(status) = self.worker_status() {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(jig_core::error::Error::Custom(format!(
                                "worker '{}' is still initializing (running on-create hook)",
                                self.branch
                            )));
                        }
                        WorkerStatus::Failed => {
                            let reason = self.fail_reason().unwrap_or("unknown".into());
                            return Err(jig_core::error::Error::Custom(format!(
                                "worker '{}' failed during setup: {}",
                                self.branch, reason
                            )));
                        }
                        _ => {}
                    }
                }
            }
            return Err(jig_core::error::Error::Custom(format!(
                "worker '{}' not found",
                self.branch
            )));
        }
        mux.attach_window(&self.branch)?;
        Ok(())
    }

    /// Run one tick: read event log, optionally check PR, enrich with runtime state.
    pub fn tick(
        &self,
        mux: &dyn Mux,
        gh: Option<&dyn GitHub>,
        config: &Config,
        repo: &RepoEntry,
    ) -> Result<WorkerState> {
        let worker_name = self.branch().to_string();

        let event_log = self.event_log()?;
        let mut state: WorkerState = event_log.reduce()?;
        state.check_silence(config);

        // PR checking + event writing
        let mut pr_health = PrHealth::default();
        let mut is_draft = false;
        let mut review_feedback_count = 0u32;

        if let Some(gh) = gh {
            if !state.status.is_terminal() {
                let info = self.check_pr(gh);
                review_feedback_count = info.review_feedback_count;
                self.process_pr_info(&info, &event_log, &state, &mut pr_health, &mut is_draft);
            }
        }

        // Re-reduce after potential PrOpened writes
        let mut state: WorkerState = event_log.reduce()?;
        state.check_silence(config);
        state.review_feedback_count = review_feedback_count;

        // Populate runtime fields
        let branch: Branch = state.branch.as_deref().unwrap_or(&worker_name).into();
        state.repo = Some(repo.clone());
        state.name = worker_name.clone();
        state.resolved_branch = branch;
        state.mux_status = self.mux_status(mux);
        let (commits_ahead, is_dirty_val) = self.git_stats(&repo.path, &worker_name);
        state.commits_ahead = commits_ahead;
        state.is_dirty = is_dirty_val;
        state.parsed_pr_url = state.pr_url.as_deref().and_then(|u| Url::parse(u).ok());
        state.pr_health = pr_health;
        state.is_draft = is_draft;
        state.max_nudges = config.max_nudges;
        state.nudge_cooldown_remaining = self.nudge_cooldown(&state, config);

        Ok(state)
    }

    fn process_pr_info(
        &self,
        info: &PrReport,
        event_log: &events::EventLog,
        state: &WorkerState,
        pr_health: &mut PrHealth,
        is_draft: &mut bool,
    ) {
        match &info.status {
            PrStatus::NoPr => {}
            PrStatus::Error { error, .. } => {
                pr_health.pr_error = Some(error.clone());
            }
            PrStatus::Merged { pr_url } | PrStatus::Closed { pr_url } => {
                pr_health.has_pr = true;
                if state.pr_url.is_none() {
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
                pr_health.has_pr = true;
                pr_health.pr_checks = checks.clone();
                *is_draft = *draft;
                if state.pr_url.is_none() {
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

    fn git_stats(&self, repo_path: &std::path::Path, worker_name: &str) -> (usize, bool) {
        let worktree_path = context::worktree_path(repo_path, worker_name);
        if !worktree_path.exists() {
            return (0, false);
        }
        let base = context::resolve_base_branch_for(repo_path)
            .unwrap_or_else(|_| Branch::new(context::DEFAULT_BASE_BRANCH));
        let ahead = Repo::open(&worktree_path)
            .and_then(|r| r.commits_ahead(&base))
            .unwrap_or_default()
            .len();
        let dirty = Repo::open(&worktree_path)
            .and_then(|r| r.has_uncommitted_changes())
            .unwrap_or(false);
        (ahead, dirty)
    }

    fn nudge_cooldown(&self, state: &WorkerState, config: &Config) -> Option<u64> {
        let now = chrono::Utc::now().timestamp();
        let mut min_remaining: Option<u64> = None;
        for &last_ts in state.last_nudge_at.values() {
            let elapsed = now - last_ts;
            if elapsed < config.silence_threshold_seconds as i64 {
                let remaining = (config.silence_threshold_seconds as i64 - elapsed) as u64;
                min_remaining = Some(min_remaining.map_or(remaining, |cur: u64| cur.min(remaining)));
            }
        }
        min_remaining
    }

    /// Check this worker's PR status via the given GitHub client.
    pub fn check_pr(&self, gh: &dyn GitHub) -> PrReport {
        let worker_key = self.worker_key();
        let branch = self.branch().to_string();

        let pr_url = match gh.get_pr_for_branch(&branch) {
            Ok(Some(pr_info)) => match Url::parse(&pr_info.url) {
                Ok(url) => url,
                Err(_) => {
                    return PrReport {
                        status: PrStatus::Error {
                            pr_url: None,
                            error: format!("invalid PR URL: {}", pr_info.url),
                        },
                        review_feedback_count: 0,
                    };
                }
            },
            Ok(None) => {
                return PrReport {
                    status: PrStatus::NoPr,
                    review_feedback_count: 0,
                }
            }
            Err(e) => {
                tracing::debug!(worker = %worker_key, error = %e, "PR discovery failed");
                return PrReport {
                    status: PrStatus::Error {
                        pr_url: None,
                        error: e.to_string(),
                    },
                    review_feedback_count: 0,
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
                return PrReport {
                    status: PrStatus::Error {
                        pr_url: Some(pr_url),
                        error: "could not parse PR number from URL".to_string(),
                    },
                    review_feedback_count: 0,
                };
            }
        };

        let pr_state_info = match gh.get_pr_state(pr_number) {
            Ok(s) => s,
            Err(e) => {
                return PrReport {
                    status: PrStatus::Error {
                        pr_url: Some(pr_url),
                        error: e.to_string(),
                    },
                    review_feedback_count: 0,
                };
            }
        };

        let status = match pr_state_info.state {
            PrState::Merged => PrStatus::Merged { pr_url },
            PrState::Closed => PrStatus::Closed { pr_url },
            PrState::Open => {
                let mut checks = PrChecks::default();
                let mut review_feedback_count: u32 = 0;

                match check_ci(gh, &branch) {
                    Ok(has_problem) => checks.ci = Some(has_problem),
                    Err(e) => tracing::debug!(error = %e, "CI check failed"),
                }
                match check_conflicts(gh, pr_number) {
                    Ok(has_problem) => checks.conflicts = Some(has_problem),
                    Err(e) => tracing::debug!(error = %e, "conflicts check failed"),
                }
                match check_reviews(gh, pr_number) {
                    Ok(r) => {
                        review_feedback_count =
                            r.review_comment_count + r.changes_requested_count;
                        checks.reviews = Some(r.has_problem);
                    }
                    Err(e) => tracing::debug!(error = %e, "reviews check failed"),
                }
                match check_commits(gh, pr_number) {
                    Ok(has_problem) => checks.commits = Some(has_problem),
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

        let review_feedback_count = match &status {
            PrStatus::Open {
                review_feedback_count,
                ..
            } => *review_feedback_count,
            _ => 0,
        };

        PrReport {
            status,
            review_feedback_count,
        }
    }

    /// Try to create a GitHub client for this worker's repo.
    pub fn github_client(&self) -> Option<GitHubClient> {
        GitHubClient::from_repo_path(self.path()).ok()
    }

    pub fn is_orphaned(&self, mux: &dyn Mux) -> bool {
        if self.has_mux_window(mux) {
            return false;
        }
        match self.worker_status() {
            Some(s) => {
                !s.is_terminal() && s != WorkerStatus::Initializing && s != WorkerStatus::Created
            }
            None => false,
        }
    }
}
