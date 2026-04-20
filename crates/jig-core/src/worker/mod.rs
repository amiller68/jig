//! Worker — the single abstraction for a Claude Code session.
//!
//! A Worker owns its identity and a [`WorktreeRef`] pointing at its
//! git worktree on disk.  The full [`Worktree`] (wrapping a git2 repo
//! handle) is resolved on demand — we never serialize what we can derive.

mod status;

pub use status::WorkerStatus;

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agents;
use crate::config::{self, JigToml};
use crate::context::RepoContext;
use crate::error::Result;
use crate::events::{Event, EventLog, EventType};
use crate::git::{Branch, DiffStats, Repo, Worktree, WorktreeRef};
use crate::global::GlobalConfig;
use crate::host::tmux::TmuxWindow;
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};

/// A reference to an issue in an external tracker (e.g. "ENG-123", "bugs/fix-auth.md").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueRef(pub String);

impl IssueRef {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for IssueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for IssueRef {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

/// Parent issue context, passed at registration time and emitted into the event log.
pub struct ParentInfo<'a> {
    pub issue: &'a str,
    pub branch: Option<&'a str>,
}

/// A Worker is a Claude Code session in an isolated git worktree.
///
/// Only identity, worktree path, and issue ref are persisted.
/// Everything else is derived at runtime via the [`Worktree`] handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) path: WorktreeRef,
    pub(crate) issue_ref: Option<IssueRef>,

    #[serde(skip)]
    pub(crate) auto_spawned: bool,
}

impl Worker {
    // ── Constructors ──

    pub fn create(
        repo: &Repo,
        branch: &Branch,
        base: &Branch,
        on_create_hook: Option<&str>,
        copy_files: &[String],
        auto: bool,
    ) -> Result<Self> {
        crate::git::ensure_excluded(&repo.common_dir(), config::JIG_DIR)?;

        let wt = Worktree::create(repo, branch, base)?;
        let repo_root = repo.clone_path();

        let wt_path = wt.path();
        if !copy_files.is_empty() {
            config::copy_worktree_files(&repo_root, &wt_path, copy_files)?;
        }

        if let Some(hook) = on_create_hook {
            config::run_on_create_hook(hook, &wt_path)?;
        }

        Ok(Self {
            id: Uuid::new_v4(),
            name: branch.to_string(),
            path: wt.as_ref(),
            issue_ref: None,
            auto_spawned: auto,
        })
    }

    pub fn open(_repo_root: &Path, worktrees_path: &Path, name: &str) -> Result<Self> {
        let path = worktrees_path.join(name);
        if !path.exists() {
            return Err(crate::error::Error::WorktreeNotFound(name.to_string()));
        }

        let _ = WorktreeRef::new(&path).open()?;

        Ok(Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            path: WorktreeRef::new(path),
            issue_ref: None,
            auto_spawned: false,
        })
    }

    pub fn list(repo_root: &Path, _worktrees_path: &Path) -> Result<Vec<Self>> {
        let repo = crate::git::Repo::open(repo_root)?;
        Ok(repo
            .list_worktrees()?
            .into_iter()
            .map(|wt| Self {
                id: Uuid::new_v4(),
                name: wt.name(),
                path: wt.as_ref(),
                issue_ref: None,
                auto_spawned: false,
            })
            .collect())
    }

    // ── Accessors ──

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn issue_ref(&self) -> Option<&IssueRef> {
        self.issue_ref.as_ref()
    }

    pub fn worktree(&self) -> Result<Worktree> {
        Ok(self.path.open()?)
    }

    // ── Tmux ──

    // TODO (cleanup): this should return Result, not swallow errors
    pub fn tmux_window(&self) -> TmuxWindow {
        let wt = self.path.open();
        let session = match wt {
            Ok(wt) => {
                let repo_name = wt.repo_name();
                format!("jig-{}", repo_name)
            }
            Err(_) => "jig-unknown".to_string(),
        };
        TmuxWindow::new(session, &self.name)
    }

    pub fn has_tmux_window(&self) -> bool {
        self.tmux_window().exists()
    }

    pub fn is_agent_running(&self) -> bool {
        self.tmux_window().is_running()
    }

    // ── Launch / Resume ──

    pub fn launch(&self, context: Option<&str>) -> Result<()> {
        let wt = self.worktree()?;
        let repo_root = wt.repo_root();

        let engine = TemplateEngine::new().with_repo(&repo_root);
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

        let config = JigToml::load(&repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

        let window = self.tmux_window();
        window.create(&self.path)?;

        let cmd = agent.spawn_command(Some(&effective_context), &config.agent.disallowed_tools);
        window.send_keys(&[&cmd, "Enter"])?;

        Ok(())
    }

    pub fn launch_wrapup(
        &self,
        context: Option<&str>,
        parent_title: &str,
        children: &[String],
    ) -> Result<()> {
        let wt = self.worktree()?;
        let repo_root = wt.repo_root();

        let engine = TemplateEngine::new().with_repo(&repo_root);
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

        let config = JigToml::load(&repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

        let window = self.tmux_window();
        window.create(&self.path)?;
        let cmd = agent.spawn_command(Some(&effective_context), &config.agent.disallowed_tools);
        window.send_keys(&[&cmd, "Enter"])?;

        Ok(())
    }

    pub fn resume(&self, context: Option<&str>) -> Result<()> {
        let wt = self.worktree()?;
        let repo_root = wt.repo_root();
        let repo_name = wt.repo_name();

        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let mut event = Event::new(EventType::Resume);
            if let Some(ctx) = context {
                event = event.with_field("context", ctx);
            }
            let _ = event_log.append(&event);
        }

        let config = JigToml::load(&repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

        let window = self.tmux_window();
        window.create(&self.path)?;
        let cmd = agent.resume_command();
        window.send_keys(&[&cmd, "Enter"])?;

        Ok(())
    }

    // ── Remove ──

    pub fn remove(&self, force: bool) -> Result<()> {
        Ok(self.worktree()?.remove(force)?)
    }

    // ── Registration (OrchestratorState) ──

    pub fn register(&self, issue_ref: Option<&str>) -> Result<()> {
        self.register_with_event(issue_ref, None, None, EventType::Spawn)
    }

    pub fn register_initializing(&self, issue_ref: Option<&str>) -> Result<()> {
        self.register_with_event(issue_ref, None, None, EventType::Initializing)
    }

    pub fn register_initializing_with_issue_text(
        &self,
        issue_ref: Option<&str>,
        issue_title: &str,
        parent: Option<ParentInfo<'_>>,
    ) -> Result<()> {
        self.register_with_event(
            issue_ref,
            Some(issue_title),
            parent,
            EventType::Initializing,
        )
    }

    fn register_with_event(
        &self,
        issue_ref: Option<&str>,
        issue_title: Option<&str>,
        parent: Option<ParentInfo<'_>>,
        initial_event_type: EventType,
    ) -> Result<()> {
        let wt = self.worktree()?;
        let repo_root = wt.repo_root();
        let branch = wt.branch()?;
        let repo_name = wt.repo_name();

        let base_branch = RepoContext::resolve_base_branch_for(&repo_root)
            .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string());

        let config = crate::config::RepoConfig {
            base_branch,
            ..Default::default()
        };

        let mut state = OrchestratorState::load_or_create(repo_root, config)?;

        let mut record = self.clone();
        record.issue_ref = issue_ref.map(IssueRef::new);

        state.add_worker(record);
        state.save()?;

        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let _ = event_log.reset();
            let branch_str: &str = &branch;
            let mut event = Event::new(initial_event_type)
                .with_field("branch", branch_str)
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
            if let Some(ref p) = parent {
                event = event.with_field("parent_issue", p.issue);
                if let Some(branch) = p.branch {
                    event = event.with_field("parent_branch", branch);
                }
            }
            let _ = event_log.append(&event);
        }

        Ok(())
    }

    pub fn emit_spawn_event(&self) {
        if let Ok(wt) = self.worktree() {
            let repo_name = wt.repo_name();
            let branch = wt
                .branch()
                .map(|b| b.to_string())
                .unwrap_or_else(|_| self.name.clone());
            if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
                let event = Event::new(EventType::Spawn)
                    .with_field("branch", branch.as_str())
                    .with_field("repo", repo_name.as_str());
                let _ = event_log.append(&event);
            }
        }
    }

    pub fn emit_setup_failed(&self, reason: &str) {
        if let Ok(wt) = self.worktree() {
            let repo_name = wt.repo_name();
            if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
                let event = Event::new(EventType::Terminal)
                    .with_field("terminal", "failed")
                    .with_field("reason", reason);
                let _ = event_log.append(&event);
            }
        }
    }

    pub fn unregister(&self) -> Result<()> {
        let wt = self.worktree()?;
        let repo_root = wt.repo_root();
        let repo_name = wt.repo_name();

        if let Some(mut state) = OrchestratorState::load(&repo_root)? {
            let id = state.get_worker_by_name(&self.name).map(|w| w.id);
            if let Some(id) = id {
                state.remove_worker(&id);
                state.save()?;
            }
        }

        if let Ok(event_log) = EventLog::for_worker(&repo_name, &self.name) {
            let _ = event_log.remove();
        }

        Ok(())
    }

    // ── Orphan detection ──

    pub fn is_orphaned(&self) -> bool {
        self.auto_spawned && !self.has_tmux_window() && self.path.exists()
    }

    // ── Git helpers (resolve worktree, delegate) ──

    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        Ok(self.worktree()?.has_uncommitted_changes()?)
    }

    pub fn get_commits_ahead(&self) -> Result<Vec<String>> {
        Ok(self.worktree()?.commits_ahead()?)
    }

    pub fn get_diff_stats(&self) -> Result<DiffStats> {
        Ok(self.worktree()?.diff_stats()?)
    }

    pub fn get_diff(&self) -> Result<String> {
        Ok(self.worktree()?.diff()?.patch()?)
    }

    pub fn get_diff_stat(&self) -> Result<String> {
        Ok(self.worktree()?.diff_stat()?)
    }

    pub fn repo_name(&self) -> String {
        self.worktree()
            .map(|wt| wt.repo_name())
            .unwrap_or_else(|_| "unknown".to_string())
    }
}
