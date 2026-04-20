//! Spawn operations for worker management
//!
//! High-level operations for spawning and managing workers.
//!
//! The [`spawn_worker_for_issue`] function is the **single authoritative codepath**
//! for daemon-driven spawning (both blocking `tick_once` and watch-mode `tick`).
//! It ensures the full sequence — create worktree → register → on-create hook →
//! spawn event → launch → update issue status — is always executed consistently.

use std::path::Path;

use crate::agents;
use crate::config::{self, JigToml, RepoConfig};
use crate::context::RepoContext;
use crate::error::{Error, Result};
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::git::WorktreeRef;
use crate::git::{Branch, Repo};
use crate::global::GlobalConfig;
use crate::host::tmux::{TmuxSession, TmuxWindow};
use crate::issues::{Issue, IssueStatus, ProviderKind};
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};
use crate::worker::{IssueRef, ParentInfo, Worker, WorkerStatus};

/// Distinguishes normal (interactive) worker spawns from wrap-up spawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpawnKind {
    /// Interactive worker in a tmux session — full tool access, persistent session.
    #[default]
    Normal,
    /// Wrap-up worker for a parent epic — spawned after all children are complete
    /// and merged. Uses the wrap-up preamble template and the parent's own
    /// integration branch as the base.
    Wrapup,
}

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
    pub issue: &'a Issue,
    pub worker_name: &'a str,
    pub provider_kind: ProviderKind,
    pub kind: SpawnKind,
}

/// Spawn a single worker for an issue: create worktree, register, run on-create
/// hook, emit spawn event, launch, and update issue status.
///
/// This is the **single authoritative codepath** for daemon-driven spawning.
/// Both `tick_once` (blocking) and `tick` (watch-mode spawn actor) call this.
pub fn spawn_worker_for_issue(input: &SpawnIssueInput<'_>) -> std::result::Result<(), String> {
    let repo_root = input.repo_root;
    let worktree_path = config::worktree_path(repo_root, input.worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %input.worker_name, "worktree already exists, skipping");
        return Ok(());
    }

    let base_branch = if input.kind == SpawnKind::Wrapup {
        // Wrap-up workers use the parent's own integration branch as base.
        // The branch already exists on origin with all child work merged in.
        match input.issue.branch_name.as_deref() {
            Some(branch) => format!("origin/{}", branch),
            None => return Err("wrap-up spawn requires issue to have a branch_name".to_string()),
        }
    } else {
        match input
            .issue
            .parent
            .as_ref()
            .and_then(|p| p.branch_name.as_deref())
        {
            Some(parent_branch) => format!("origin/{}", parent_branch),
            None => RepoContext::resolve_base_branch_for(repo_root)
                .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string()),
        }
    };

    let copy_files = config::get_copy_files(repo_root).map_err(|e| e.to_string())?;
    let on_create_hook = config::get_on_create_hook(repo_root).map_err(|e| e.to_string())?;
    let repo = Repo::open(repo_root).map_err(|e| e.to_string())?;

    // Create worktree WITHOUT running on-create hook — we handle it after registration
    let branch = Branch::new(
        input
            .issue
            .branch_name
            .as_deref()
            .unwrap_or(input.worker_name),
    );
    let base = Branch::new(&base_branch);
    let wt = Worker::create(
        &repo,
        &branch,
        &base,
        None, // defer on-create hook
        &copy_files,
        true, // auto_spawned
    )
    .map_err(|e| e.to_string())?;

    let parent_info = input.issue.parent.as_ref().map(|p| ParentInfo {
        issue: &p.id,
        branch: p.branch_name.as_deref(),
    });

    let context = input.issue.to_spawn_context(input.provider_kind);

    wt.register_initializing_with_issue_text(
        Some(&input.issue.id),
        &input.issue.title,
        parent_info,
    )
    .map_err(|e| e.to_string())?;

    // Run on-create hook now that the worker is visible
    if let Some(hook) = on_create_hook.as_deref() {
        let success = config::run_on_create_hook(hook, wt.path()).map_err(|e| e.to_string())?;
        if !success {
            wt.emit_setup_failed("on-create hook failed");
            return Err("on-create hook failed".to_string());
        }
    }

    // Transition from Initializing → Spawned
    wt.emit_spawn_event();

    match input.kind {
        SpawnKind::Normal => {
            wt.launch(Some(&context)).map_err(|e| e.to_string())?;
            // Update issue status to InProgress to prevent duplicate spawning
            update_issue_status(repo_root, &input.issue.id);
        }
        SpawnKind::Wrapup => {
            let child_ids: Vec<String> =
                input.issue.children.iter().map(|c| c.id.clone()).collect();
            wt.launch_wrapup(Some(&context), &input.issue.title, &child_ids)
                .map_err(|e| e.to_string())?;
            // Parent is already InProgress — no status update needed
        }
    }

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

/// Render the triage prompt for an issue.
///
/// Returns the rendered prompt text. Used by both the `triage_actor` (subprocess)
/// and any inline triage path.
pub fn render_triage_prompt(repo_root: &Path, issue: &Issue) -> Result<String> {
    let engine = TemplateEngine::new().with_repo(repo_root);
    let mut tpl_ctx = TemplateContext::new();
    tpl_ctx.set("issue_id", &issue.id);
    tpl_ctx.set("issue_title", &issue.title);
    tpl_ctx.set("issue_body", &issue.body);
    tpl_ctx.set_list("issue_labels", issue.labels.clone());

    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    tpl_ctx.set("repo_name", &repo_name);

    engine.render("triage-prompt", &tpl_ctx)
}

/// Run a triage worker as a direct subprocess (blocking).
///
/// Renders the triage prompt, builds the argv, and executes Claude Code
/// with stdin piped from the prompt. Returns `Ok(())` on success or an
/// error message on failure.
pub fn run_triage_subprocess(repo_root: &Path, issue: &Issue) -> std::result::Result<(), String> {
    // Allowed tools for triage workers — read-only codebase access plus jig CLI.
    const TRIAGE_ALLOWED_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Bash(jig *)"];

    let prompt = render_triage_prompt(repo_root, issue).map_err(|e| e.to_string())?;

    let jig_toml = config::JigToml::load(repo_root)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let model = &jig_toml.triage.model;
    let agent = agents::Agent::from_name(&jig_toml.agent.agent_type)
        .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

    let argv = agent.triage_argv(model, TRIAGE_ALLOWED_TOOLS);

    // argv[0] is the command, rest are args
    let (cmd, args) = argv.split_first().ok_or("empty triage argv")?;

    let output = std::process::Command::new(cmd)
        .args(args)
        .current_dir(repo_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            // Drop stdin to signal EOF
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .map_err(|e| format!("failed to execute triage agent: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "triage agent exited with {}: {}",
            output.status,
            stderr.chars().take(500).collect::<String>()
        ));
    }

    Ok(())
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
    issue_ref: Option<&str>,
) -> Result<()> {
    let worktree_path = repo.worktrees_path.join(name);

    let config = RepoConfig {
        base_branch: repo.base_branch.clone(),
        ..Default::default()
    };

    let mut state = OrchestratorState::load_or_create(repo.repo_root.clone(), config)?;

    let worker = Worker {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        path: WorktreeRef::new(worktree_path),
        issue_ref: issue_ref.map(IssueRef::new),
        auto_spawned: false,
    };
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
    let agent = agents::Agent::from_name(&config.agent.agent_type)
        .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

    let window = TmuxWindow::new(&repo.session_name, name);
    window.create(worktree_path)?;

    let cmd = agent.spawn_command(Some(&effective_context), &config.agent.disallowed_tools);
    window.send_keys(&[&cmd, "Enter"])?;

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
            let worktree_path = repo.worktrees_path.join(&worker.name);

            let (commits_ahead, is_dirty) = if worktree_path.exists() {
                let commits = Repo::open(&worktree_path)
                    .and_then(|r| r.commits_ahead(&Branch::new(&repo.base_branch)))
                    .unwrap_or_default()
                    .len();
                let dirty = Repo::open(&worktree_path)
                    .and_then(|r| r.has_uncommitted_changes())
                    .unwrap_or(false);
                (commits, dirty)
            } else {
                (0, false)
            };

            let issue_ref = worker.issue_ref.as_ref().map(|r| r.0.clone());

            tasks.push(TaskInfo {
                name: worker.name.clone(),
                status,
                branch: Repo::open(&worktree_path)?
                    .current_branch()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|_| worker.name.clone()),
                commits_ahead,
                is_dirty,
                issue_ref,
            });
        }
    } else {
        // Fall back to checking tmux windows directly
        let session = TmuxSession::new(&repo.session_name);
        let windows = session.window_names()?;

        for window_name in windows {
            let worktree_path = repo.worktrees_path.join(&window_name);
            if !worktree_path.exists() {
                continue;
            }

            let status = get_worker_status(&repo.session_name, &window_name);
            let branch = Repo::open(&worktree_path)?
                .current_branch()
                .map(|b| b.to_string())
                .unwrap_or_default();

            let commits_ahead = Repo::open(&worktree_path)
                .and_then(|r| r.commits_ahead(&Branch::new(&repo.base_branch)))
                .unwrap_or_default()
                .len();
            let is_dirty = Repo::open(&worktree_path)
                .and_then(|r| r.has_uncommitted_changes())
                .unwrap_or(false);

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

fn get_worker_status(session_name: &str, window_name: &str) -> TaskStatus {
    let session = TmuxSession::new(session_name);
    if !session.exists() {
        return TaskStatus::NoSession;
    }
    let window = session.window(window_name);
    if !window.exists() {
        return TaskStatus::NoWindow;
    }
    if window.is_running() {
        TaskStatus::Running
    } else {
        TaskStatus::Exited
    }
}

/// Attach to tmux session
pub fn attach(repo: &RepoContext, name: Option<&str>) -> Result<()> {
    let session = TmuxSession::new(&repo.session_name);
    if let Some(window_name) = name {
        let window = session.window(window_name);
        if !window.exists() {
            let worktree_path = repo.worktrees_path.join(window_name);
            if worktree_path.exists() {
                if let Some(status) = get_worker_event_status(repo, window_name) {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(Error::WorkerInitializing(window_name.to_string()));
                        }
                        WorkerStatus::Failed => {
                            return Err(Error::WorkerSetupFailed(
                                window_name.to_string(),
                                get_worker_fail_reason(repo, window_name)
                                    .unwrap_or_else(|| "unknown".to_string()),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            return Err(Error::WorkerNotFound(window_name.to_string()));
        }
        window.attach()?;
        Ok(())
    } else {
        session.attach()?;
        Ok(())
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
    let window = TmuxWindow::new(&repo.session_name, name);
    window.kill()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_kind_default_is_normal() {
        let kind: SpawnKind = Default::default();
        assert_eq!(kind, SpawnKind::Normal);
    }

    #[test]
    fn spawn_kind_equality() {
        assert_eq!(SpawnKind::Normal, SpawnKind::Normal);
        assert_eq!(SpawnKind::Wrapup, SpawnKind::Wrapup);
        assert_ne!(SpawnKind::Normal, SpawnKind::Wrapup);
    }
}
