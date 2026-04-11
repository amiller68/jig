//! Worktree operations
//!
//! `Worktree` is the single abstraction for a worker's physical state —
//! its repo, branch, path, tmux session, spawn context, and lifecycle.

use std::path::{Path, PathBuf};

use crate::adapter;
use crate::config::{self, copy_worktree_files, run_on_create_hook, JigToml, RepoConfig};
use crate::context::RepoContext;
use crate::error::{Error, Result};
use crate::events::{Event, EventLog, EventType};
use crate::git::{self, Repo};
use crate::global::GlobalConfig;
use crate::session;
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};
use crate::worker::{TaskContext, Worker};

/// Represents a git worktree — the single source of truth for a worker's physical state.
#[derive(Debug, Clone)]
pub struct Worktree {
    /// Name of the worktree (relative path from .jig/, e.g. "features/global-attach")
    pub name: String,
    /// Full path to the worktree
    pub path: PathBuf,
    /// Branch name
    pub branch: String,
    /// Parent repo root
    pub repo_root: PathBuf,
    /// Tmux session name (e.g. "jig-<repo>")
    pub session_name: String,
    /// Whether this worktree was daemon-created vs manual
    pub auto_spawned: bool,
    /// Parent issue ID if this worktree is a child of another issue.
    pub parent_issue: Option<String>,
    /// Parent issue's branch name (the base branch this child was forked from).
    pub parent_branch: Option<String>,
}

impl Worktree {
    /// Create a new worktree on disk, returning a populated struct.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        repo_root: &Path,
        worktrees_dir: &Path,
        git_common_dir: &Path,
        name: &str,
        branch: Option<&str>,
        base_branch: &str,
        on_create_hook: Option<&str>,
        copy_files: &[String],
        auto: bool,
    ) -> Result<Self> {
        let worktree_path = worktrees_dir.join(name);

        // Check if already exists
        if worktree_path.exists() {
            return Err(Error::WorktreeExists(name.to_string()));
        }

        // Ensure .jig is gitignored
        git::ensure_worktrees_excluded(git_common_dir)?;

        // Create parent directories if needed (for nested paths like feature/auth/login)
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Determine branch name
        let branch = branch.unwrap_or(name);

        // Create the worktree — use Repo::open, not Repo::discover
        let repo = Repo::open(repo_root)?;
        repo.create_worktree(&worktree_path, branch, base_branch)?;

        // Copy configured files (e.g., .env)
        if !copy_files.is_empty() {
            copy_worktree_files(repo_root, &worktree_path, copy_files)?;
        }

        // Run on-create hook if configured
        if let Some(hook) = on_create_hook {
            run_on_create_hook(hook, &worktree_path)?;
        }

        let session_name = Self::derive_session_name(repo_root);

        Ok(Self {
            name: name.to_string(),
            path: worktree_path,
            branch: branch.to_string(),
            repo_root: repo_root.to_path_buf(),
            session_name,
            auto_spawned: auto,
            parent_issue: None,
            parent_branch: None,
        })
    }

    /// List all worktrees in a directory.
    pub fn list(repo_root: &Path, worktrees_dir: &Path) -> Result<Vec<Self>> {
        let names = git::list_worktree_names(worktrees_dir)?;
        let session_name = Self::derive_session_name(repo_root);

        names
            .into_iter()
            .map(|name| {
                let path = worktrees_dir.join(&name);
                let branch = Repo::worktree_branch(&path).unwrap_or_else(|_| name.clone());
                Ok(Self {
                    name,
                    path,
                    branch,
                    repo_root: repo_root.to_path_buf(),
                    session_name: session_name.clone(),
                    auto_spawned: false,
                    parent_issue: None,
                    parent_branch: None,
                })
            })
            .collect()
    }

    /// Open/get an existing worktree by name.
    pub fn open(repo_root: &Path, worktrees_dir: &Path, name: &str) -> Result<Self> {
        let path = worktrees_dir.join(name);

        if !path.exists() {
            return Err(Error::WorktreeNotFound(name.to_string()));
        }

        let branch = Repo::worktree_branch(&path)?;
        let session_name = Self::derive_session_name(repo_root);

        Ok(Self {
            name: name.to_string(),
            path,
            branch,
            repo_root: repo_root.to_path_buf(),
            session_name,
            auto_spawned: false,
            parent_issue: None,
            parent_branch: None,
        })
    }

    /// Remove this worktree. Uses `Repo::open(repo_root)`, never `Repo::discover()`.
    pub fn remove(&self, force: bool) -> Result<()> {
        // Check for uncommitted changes unless force
        if !force && Repo::has_uncommitted_changes(&self.path)? {
            return Err(Error::UncommittedChanges);
        }

        Repo::remove_worktree(&self.path, force, Some(&self.repo_root))?;

        // Clean up empty parent directories (for nested paths)
        self.cleanup_empty_parents()?;

        Ok(())
    }

    // ---------------------------------------------------------------
    // Tmux methods
    // ---------------------------------------------------------------

    /// Check if this worktree has a tmux window.
    pub fn has_tmux_window(&self) -> bool {
        session::window_exists(&self.session_name, &self.name)
    }

    /// Check if the agent is running in the tmux pane.
    pub fn is_agent_running(&self) -> bool {
        session::pane_is_running(&self.session_name, &self.name)
    }

    /// Launch a tmux window for this worktree using the wrap-up preamble.
    ///
    /// Used for parent epic workers spawned after all children are complete
    /// and merged into the parent integration branch. The wrap-up preamble
    /// renders the parent title and the list of completed child IDs.
    pub fn launch_wrapup(
        &self,
        context: Option<&str>,
        parent_title: &str,
        children: &[String],
    ) -> Result<()> {
        let engine = TemplateEngine::new().with_repo(&self.repo_root);
        let global_config = GlobalConfig::load()?;
        let mut tpl_ctx = TemplateContext::new();
        tpl_ctx.set_num("max_nudges", global_config.health.max_nudges);
        tpl_ctx.set("parent_title", parent_title);
        tpl_ctx.set(
            "task_context",
            context.unwrap_or(
                "No specific task provided. Check CLAUDE.md and the issue tracker for context.",
            ),
        );
        tpl_ctx.set_list("children", children.to_vec());
        let effective_context = engine.render("spawn-preamble-wrapup", &tpl_ctx)?;

        let config = JigToml::load(&self.repo_root)?.unwrap_or_default();
        let agent_adapter =
            adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

        session::create_window(&self.session_name, &self.name, &self.path)?;
        let cmd = adapter::build_spawn_command(agent_adapter, Some(&effective_context));
        session::send_keys(&self.session_name, &self.name, &cmd)?;

        Ok(())
    }

    /// Launch a tmux window for this worktree (always uses auto mode).
    pub fn launch(&self, context: Option<&str>) -> Result<()> {
        // Always render preamble
        let engine = TemplateEngine::new().with_repo(&self.repo_root);
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

        // Get adapter from config
        let config = JigToml::load(&self.repo_root)?.unwrap_or_default();
        let agent_adapter =
            adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

        // Create window in tmux
        session::create_window(&self.session_name, &self.name, &self.path)?;

        // Build spawn command using adapter (always auto)
        let cmd = adapter::build_spawn_command(agent_adapter, Some(&effective_context));

        // Send command to window
        session::send_keys(&self.session_name, &self.name, &cmd)?;

        Ok(())
    }

    /// Resume this worktree by continuing the agent's prior session.
    ///
    /// For adapters that support session continuation (e.g. Claude Code's `-c` flag),
    /// this picks up the most recent session transcript instead of starting fresh.
    /// For adapters without continuation support, falls back to a fresh launch with
    /// the provided context.
    pub fn resume(&self, context: Option<&str>) -> Result<()> {
        let repo_name = self.repo_name();

        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let mut event = Event::new(EventType::Resume);
            if let Some(ctx) = context {
                event = event.with_field("context", ctx);
            }
            let _ = event_log.append(&event);
        }

        // Get adapter from config
        let config = JigToml::load(&self.repo_root)?.unwrap_or_default();
        let agent_adapter =
            adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

        if !agent_adapter.supports_continue() {
            // Adapter doesn't support session continuation — fall back to re-spawn
            return self.launch(context);
        }

        // Continue the prior session via the adapter's continue flag
        session::create_window(&self.session_name, &self.name, &self.path)?;
        let cmd = adapter::build_resume_command(agent_adapter);
        session::send_keys(&self.session_name, &self.name, &cmd)?;

        Ok(())
    }

    // ---------------------------------------------------------------
    // Registration (absorb from spawn.rs)
    // ---------------------------------------------------------------

    /// Register this worktree as a worker in the orchestrator state,
    /// emitting an Initializing event instead of Spawn. Use this when the
    /// worker needs to run an on-create hook before being fully ready.
    pub fn register_initializing(
        &self,
        context: Option<&str>,
        issue_ref: Option<&str>,
    ) -> Result<()> {
        self.register_with_event(context, issue_ref, None, EventType::Initializing)
    }

    /// Register this worktree as a worker in the orchestrator state,
    /// emitting an Initializing event with the issue title stored in the event data.
    pub fn register_initializing_with_issue_text(
        &self,
        context: Option<&str>,
        issue_ref: Option<&str>,
        issue_title: &str,
    ) -> Result<()> {
        self.register_with_event(
            context,
            issue_ref,
            Some(issue_title),
            EventType::Initializing,
        )
    }

    /// Register this worktree as a worker in the orchestrator state.
    pub fn register(&self, context: Option<&str>, issue_ref: Option<&str>) -> Result<()> {
        self.register_with_event(context, issue_ref, None, EventType::Spawn)
    }

    fn register_with_event(
        &self,
        context: Option<&str>,
        issue_ref: Option<&str>,
        issue_title: Option<&str>,
        initial_event_type: EventType,
    ) -> Result<()> {
        let config = RepoConfig {
            base_branch: self.resolve_base_branch(),
            ..Default::default()
        };

        let mut state = OrchestratorState::load_or_create(self.repo_root.clone(), config)?;

        let mut worker = Worker::new(
            self.name.clone(),
            self.path.clone(),
            self.branch.clone(),
            self.resolve_base_branch(),
            self.session_name.clone(),
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

        worker.tmux_window = Some(self.name.clone());
        state.add_worker(worker);
        state.save()?;

        // Emit initial event (Spawn or Initializing)
        let repo_name = self.repo_name();

        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let _ = event_log.reset();
            let mut event = Event::new(initial_event_type)
                .with_field("branch", self.branch.as_str())
                .with_field("repo", repo_name.as_str());
            if self.auto_spawned {
                event = event.with_field("auto", true);
            }
            if let Some(issue) = issue_ref {
                event = event.with_field("issue", issue);
            }
            if let Some(title) = issue_title {
                event = event.with_field("issue_title", title);
            }
            if let Some(ref pi) = self.parent_issue {
                event = event.with_field("parent_issue", pi.as_str());
            }
            if let Some(ref pb) = self.parent_branch {
                event = event.with_field("parent_branch", pb.as_str());
            }
            let _ = event_log.append(&event);
        }

        Ok(())
    }

    /// Emit a Spawn event, transitioning from Initializing to Spawned.
    pub fn emit_spawn_event(&self) {
        let repo_name = self.repo_name();
        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let event = Event::new(EventType::Spawn)
                .with_field("branch", self.branch.as_str())
                .with_field("repo", repo_name.as_str());
            let _ = event_log.append(&event);
        }
    }

    /// Emit a Terminal "failed" event with a reason.
    pub fn emit_setup_failed(&self, reason: &str) {
        let repo_name = self.repo_name();
        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let event = Event::new(EventType::Terminal)
                .with_field("terminal", "failed")
                .with_field("reason", reason);
            let _ = event_log.append(&event);
        }
    }

    /// Unregister this worktree from state and clean up event log.
    pub fn unregister(&self) -> Result<()> {
        if let Some(mut state) = OrchestratorState::load(&self.repo_root)? {
            let id = state.get_worker_by_name(&self.name).map(|w| w.id);
            if let Some(id) = id {
                state.remove_worker(&id);
                state.save()?;
            }
        }

        // Clean up event log
        let repo_name = self.repo_name();
        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let _ = event_log.remove();
        }

        Ok(())
    }

    // ---------------------------------------------------------------
    // Orphan detection
    // ---------------------------------------------------------------

    /// An orphaned worktree is auto-spawned, has no tmux window, but still exists on disk.
    pub fn is_orphaned(&self) -> bool {
        self.auto_spawned && !self.has_tmux_window() && self.path.exists()
    }

    // ---------------------------------------------------------------
    // Diff/status helpers (kept for compatibility)
    // ---------------------------------------------------------------

    /// Check if this worktree has uncommitted changes
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        Repo::has_uncommitted_changes(&self.path)
    }

    /// Get commits ahead of base branch
    pub fn get_commits_ahead(&self, base_branch: &str) -> Result<Vec<String>> {
        Repo::commits_ahead(&self.path, base_branch)
    }

    /// Get diff stats
    pub fn get_diff_stats(&self, base_branch: &str) -> Result<crate::worker::DiffStats> {
        Repo::diff_stats(&self.path, base_branch)
    }

    /// Get full diff
    pub fn get_diff(&self, base_branch: &str) -> Result<String> {
        Repo::diff(&self.path, base_branch)
    }

    /// Get diff stat (summary)
    pub fn get_diff_stat(&self, base_branch: &str) -> Result<String> {
        Repo::diff_stat(&self.path, base_branch)
    }

    // ---------------------------------------------------------------
    // Private helpers
    // ---------------------------------------------------------------

    /// Clean up empty parent directories.
    fn cleanup_empty_parents(&self) -> Result<()> {
        let mut parent = self.path.parent();

        while let Some(p) = parent {
            // Stop if we've reached the jig directory
            if p.file_name().map(|n| n == config::JIG_DIR).unwrap_or(false) {
                break;
            }

            // Stop if directory is not empty
            if p.read_dir()?.next().is_some() {
                break;
            }

            std::fs::remove_dir(p)?;
            parent = p.parent();
        }

        Ok(())
    }

    fn repo_name(&self) -> String {
        self.repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn derive_session_name(repo_root: &Path) -> String {
        let repo_name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        format!("jig-{}", repo_name)
    }

    fn resolve_base_branch(&self) -> String {
        RepoContext::resolve_base_branch_for(&self.repo_root)
            .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string())
    }
}
