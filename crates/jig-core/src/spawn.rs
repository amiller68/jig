//! Spawn operations for worker management
//!
//! High-level operations for spawning and managing workers.
//!
//! The [`spawn_worker_for_issue`] function is the **single authoritative codepath**
//! for daemon-driven spawning (both blocking `tick_once` and watch-mode `tick`).
//! It ensures the full sequence — create worktree → register → on-create hook →
//! spawn event → launch → update issue status — is always executed consistently.

use std::path::Path;

use crate::adapter;
use crate::config::{self, JigToml, RepoConfig};
use crate::context::RepoContext;
use crate::error::{Error, Result};
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::git::Repo;
use crate::global::GlobalConfig;
use crate::issues::{IssueStatus, ProviderKind};
use crate::session;
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};
use crate::worker::{TaskContext, Worker, WorkerStatus};
use crate::worktree::Worktree;

/// Task status for ps command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Exited,
    NoSession,
    NoWindow,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Running => "running",
            TaskStatus::Exited => "exited",
            TaskStatus::NoSession => "no-session",
            TaskStatus::NoWindow => "no-window",
        }
    }
}

/// Input for the shared [`spawn_worker_for_issue`] function.
///
/// Mirrors the fields from `daemon::messages::SpawnableIssue` that are needed
/// for the spawn sequence. Daemon code converts `SpawnableIssue` → `SpawnIssueInput`.
pub struct SpawnIssueInput<'a> {
    pub repo_root: &'a Path,
    pub issue_id: &'a str,
    pub issue_title: &'a str,
    pub issue_body: &'a str,
    pub worker_name: &'a str,
    pub provider_kind: ProviderKind,
    pub branch_name: Option<&'a str>,
}

/// Spawn a single worker for an issue: create worktree, register, run on-create
/// hook, emit spawn event, launch, and update issue status.
///
/// This is the **single authoritative codepath** for daemon-driven spawning.
/// Both `tick_once` (blocking) and `tick` (watch-mode spawn actor) call this.
pub fn spawn_worker_for_issue(input: &SpawnIssueInput<'_>) -> std::result::Result<(), String> {
    let repo_root = input.repo_root;
    let worktrees_dir = repo_root.join(config::JIG_DIR);
    let worktree_path = config::worktree_path(repo_root, input.worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %input.worker_name, "worktree already exists, skipping");
        return Ok(());
    }

    let base_branch = RepoContext::resolve_base_branch_for(repo_root)
        .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string());

    let copy_files = config::get_copy_files(repo_root).map_err(|e| e.to_string())?;
    let on_create_hook = config::get_on_create_hook(repo_root).map_err(|e| e.to_string())?;
    let git_common_dir = Repo::open(repo_root)
        .map_err(|e| e.to_string())?
        .common_dir();

    // Create worktree WITHOUT running on-create hook — we handle it after registration
    let wt = Worktree::create(
        repo_root,
        &worktrees_dir,
        &git_common_dir,
        input.worker_name,
        input.branch_name,
        &base_branch,
        None, // defer on-create hook
        &copy_files,
        true, // auto_spawned
    )
    .map_err(|e| e.to_string())?;

    let context = build_issue_context(input);

    // Register as Initializing so jig ps/ls show the worker immediately,
    // injecting the issue title into the event data for later retrieval.
    wt.register_initializing_with_issue_text(
        Some(&context),
        Some(input.issue_id),
        input.issue_title,
    )
    .map_err(|e| e.to_string())?;

    // Run on-create hook now that the worker is visible
    if let Some(hook) = on_create_hook.as_deref() {
        let success = config::run_on_create_hook(hook, &wt.path).map_err(|e| e.to_string())?;
        if !success {
            wt.emit_setup_failed("on-create hook failed");
            return Err("on-create hook failed".to_string());
        }
    }

    // Transition from Initializing → Spawned
    wt.emit_spawn_event();

    wt.launch(Some(&context)).map_err(|e| e.to_string())?;

    // Update issue status to InProgress to prevent duplicate spawning
    update_issue_status(repo_root, input.issue_id);

    Ok(())
}

/// Update an issue's status to InProgress after spawning.
///
/// Logs warnings on failure but never propagates errors — spawning should
/// succeed even if the status update fails.
pub fn update_issue_status(repo_root: &Path, issue_id: &str) {
    let ctx = match RepoContext::from_path(repo_root) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::warn!(issue = %issue_id, error = %e, "failed to load repo context for issue status update");
            return;
        }
    };
    match ctx.issue_provider() {
        Ok(provider) => {
            if let Err(e) = provider.update_status(issue_id, &IssueStatus::InProgress) {
                tracing::warn!(
                    issue = %issue_id,
                    error = %e,
                    "failed to update issue status to InProgress"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                issue = %issue_id,
                error = %e,
                "failed to create provider for issue status update"
            );
        }
    }
}

/// Build the context string for a spawned worker from issue details.
fn build_issue_context(input: &SpawnIssueInput<'_>) -> String {
    let completion_instructions = match input.provider_kind {
        ProviderKind::File => format!(
            "\n\nISSUE COMPLETION: This issue is tracked by the file provider. \
             After your PR is created, mark the issue as done by changing \
             `**Status:** Planned` to `**Status:** Complete` in the issue file \
             (`issues/{}.md`) and committing the change.",
            input.issue_id
        ),
        ProviderKind::Linear => "\n\nISSUE COMPLETION: This issue is tracked by Linear. \
             Status sync is handled automatically — no manual status update is needed."
            .to_string(),
    };
    format!(
        "{}\n\n{}{}",
        input.issue_title, input.issue_body, completion_instructions
    )
}

/// Task info for ps command
#[derive(Debug)]
pub struct TaskInfo {
    pub name: String,
    pub status: TaskStatus,
    pub branch: String,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub issue_ref: Option<String>,
}

/// Register a new spawn (creates worker state)
pub fn register(
    repo: &RepoContext,
    name: &str,
    branch: &str,
    context: Option<&str>,
    issue_ref: Option<&str>,
) -> Result<()> {
    let worktree_path = repo.worktrees_dir.join(name);

    let config = RepoConfig {
        base_branch: repo.base_branch.clone(),
        ..Default::default()
    };

    let mut state = OrchestratorState::load_or_create(repo.repo_root.clone(), config)?;

    let mut worker = Worker::new(
        name.to_string(),
        worktree_path,
        branch.to_string(),
        repo.base_branch.clone(),
        repo.session_name.clone(),
    );

    // Set task context if provided
    if let Some(ctx) = context {
        let mut task = TaskContext::new(ctx.to_string());
        if let Some(issue) = issue_ref {
            task = task.with_issue(issue.to_string());
        }
        worker.set_task(task);
    } else if let Some(issue) = issue_ref {
        worker.set_task(TaskContext::new(String::new()).with_issue(issue.to_string()));
    }

    worker.tmux_window = Some(name.to_string());
    state.add_worker(worker);
    state.save()?;

    // Emit Spawn event so daemon and ps --watch can discover this worker.
    // Reset the log first — if a previous worker had this name, start fresh.
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if let Ok(event_log) = EventLog::for_worker(&repo_name, name) {
        let _ = event_log.reset();
        let mut event = Event::new(EventType::Spawn)
            .with_field("branch", branch)
            .with_field("repo", repo_name.as_str());
        if let Some(issue) = issue_ref {
            event = event.with_field("issue", issue);
        }
        let _ = event_log.append(&event);
    }

    Ok(())
}

/// Launch a tmux window for a worker (always uses auto mode)
pub fn launch_tmux_window(
    repo: &RepoContext,
    name: &str,
    worktree_path: &Path,
    context: Option<&str>,
) -> Result<()> {
    // Always render preamble
    let engine = TemplateEngine::new().with_repo(&repo.repo_root);
    let global_config = GlobalConfig::load()?;
    let mut tpl_ctx = TemplateContext::new();
    tpl_ctx.set_num("max_nudges", global_config.health.max_nudges);
    tpl_ctx.set(
        "task_context",
        context.unwrap_or(
            "No specific task provided. Check CLAUDE.md and the issue tracker for context.",
        ),
    );
    let effective_context = engine.render("spawn-preamble", &tpl_ctx)?;

    // Get adapter from config (fallback to claude-code if not configured)
    let config = JigToml::load(&repo.repo_root)?.unwrap_or_default();
    let agent_adapter =
        adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

    // Create window in tmux
    session::create_window(&repo.session_name, name, worktree_path)?;

    // Build spawn command using adapter (always auto)
    let cmd = adapter::build_spawn_command(agent_adapter, Some(&effective_context));

    // Send command to window
    session::send_keys(&repo.session_name, name, &cmd)?;

    Ok(())
}

/// List all tasks (workers) with their status
pub fn list_tasks(repo: &RepoContext) -> Result<Vec<TaskInfo>> {
    // Clean up stale workers and get the (already loaded) state
    let state = cleanup_stale_workers(&repo.repo_root, &repo.session_name)?;

    let mut tasks = Vec::new();

    // If we have state, use workers from state
    if let Some(state) = state {
        for worker in state.workers.values() {
            let status = get_worker_status(&repo.session_name, &worker.name);
            let worktree_path = repo.worktrees_dir.join(&worker.name);

            let (commits_ahead, is_dirty) = if worktree_path.exists() {
                let commits = Repo::commits_ahead(&worktree_path, &repo.base_branch)
                    .unwrap_or_default()
                    .len();
                let dirty = Repo::has_uncommitted_changes(&worktree_path).unwrap_or(false);
                (commits, dirty)
            } else {
                (0, false)
            };

            let issue_ref = worker.task.as_ref().and_then(|t| t.issue_ref.clone());

            tasks.push(TaskInfo {
                name: worker.name.clone(),
                status,
                branch: worker.branch.clone(),
                commits_ahead,
                is_dirty,
                issue_ref,
            });
        }
    } else {
        // Fall back to checking tmux windows directly
        let windows = session::list_windows(&repo.session_name)?;

        for window_name in windows {
            let worktree_path = repo.worktrees_dir.join(&window_name);
            if !worktree_path.exists() {
                continue;
            }

            let status = get_worker_status(&repo.session_name, &window_name);
            let branch = Repo::worktree_branch(&worktree_path).unwrap_or_default();

            let commits_ahead = Repo::commits_ahead(&worktree_path, &repo.base_branch)
                .unwrap_or_default()
                .len();
            let is_dirty = Repo::has_uncommitted_changes(&worktree_path).unwrap_or(false);

            tasks.push(TaskInfo {
                name: window_name,
                status,
                branch,
                commits_ahead,
                is_dirty,
                issue_ref: None,
            });
        }
    }

    tasks.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(tasks)
}

fn get_worker_status(session: &str, window: &str) -> TaskStatus {
    if !session::session_exists(session) {
        return TaskStatus::NoSession;
    }

    if !session::window_exists(session, window) {
        return TaskStatus::NoWindow;
    }

    if session::pane_is_running(session, window) {
        TaskStatus::Running
    } else {
        TaskStatus::Exited
    }
}

/// Attach to tmux session
pub fn attach(repo: &RepoContext, name: Option<&str>) -> Result<()> {
    if let Some(window) = name {
        if !session::window_exists(&repo.session_name, window) {
            // Check if the worker is initializing or failed
            let worktree_path = repo.worktrees_dir.join(window);
            if worktree_path.exists() {
                if let Some(status) = get_worker_event_status(repo, window) {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(Error::WorkerInitializing(window.to_string()));
                        }
                        WorkerStatus::Failed => {
                            return Err(Error::WorkerSetupFailed(
                                window.to_string(),
                                get_worker_fail_reason(repo, window)
                                    .unwrap_or_else(|| "unknown".to_string()),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            return Err(Error::WorkerNotFound(window.to_string()));
        }
        // Attach directly to session:window — doesn't change other clients' active window
        session::attach_window(&repo.session_name, window)
    } else {
        session::attach(&repo.session_name)
    }
}

/// Derive worker status from event log.
fn get_worker_event_status(repo: &RepoContext, name: &str) -> Option<WorkerStatus> {
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let event_log = EventLog::for_worker(&repo_name, name).ok()?;
    let events = event_log.read_all().ok()?;
    if events.is_empty() {
        return None;
    }
    let config = GlobalConfig::load().ok()?.health;
    Some(WorkerState::reduce(&events, &config).status)
}

/// Get the failure reason from a worker's event log.
fn get_worker_fail_reason(repo: &RepoContext, name: &str) -> Option<String> {
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let event_log = EventLog::for_worker(&repo_name, name).ok()?;
    let events = event_log.read_all().ok()?;
    events.iter().rev().find_map(|e| {
        e.data
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

/// Kill a worker's tmux window (without updating state)
pub fn kill_window(repo: &RepoContext, name: &str) -> Result<()> {
    session::kill_window(&repo.session_name, name)?;
    Ok(())
}

/// Unregister a worker from state (removes entirely) and clean up event log.
pub fn unregister(repo: &RepoContext, name: &str) -> Result<()> {
    if let Some(mut state) = OrchestratorState::load(&repo.repo_root)? {
        let id = state.get_worker_by_name(name).map(|w| w.id);
        if let Some(id) = id {
            state.remove_worker(&id);
            state.save()?;
        }
    }

    // Clean up event log
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if let Ok(event_log) = EventLog::for_worker(&repo_name, name) {
        let _ = event_log.remove();
    }

    Ok(())
}

/// Remove stale workers (whose tmux windows no longer exist) from state.
/// Preserves workers that are Initializing or Failed (they have no tmux window by design).
/// Returns the cleaned state if one existed.
fn cleanup_stale_workers(
    repo_root: &std::path::Path,
    session_name: &str,
) -> Result<Option<OrchestratorState>> {
    let mut state = match OrchestratorState::load(repo_root)? {
        Some(s) => s,
        None => return Ok(None),
    };

    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let health_config = GlobalConfig::load().map(|g| g.health).unwrap_or_default();

    let stale_ids: Vec<_> = state
        .workers
        .values()
        .filter(|w| {
            let tmux_status = get_worker_status(session_name, &w.name);
            if !matches!(tmux_status, TaskStatus::NoWindow | TaskStatus::NoSession) {
                return false;
            }
            // Don't clean up workers that are initializing or failed —
            // they intentionally have no tmux window
            if let Ok(event_log) = EventLog::for_worker(&repo_name, &w.name) {
                if let Ok(events) = event_log.read_all() {
                    if !events.is_empty() {
                        let ws = WorkerState::reduce(&events, &health_config);
                        if matches!(ws.status, WorkerStatus::Initializing | WorkerStatus::Failed) {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .map(|w| w.id)
        .collect();

    if !stale_ids.is_empty() {
        for id in &stale_ids {
            state.remove_worker(id);
        }
        state.save()?;
    }

    Ok(Some(state))
}
