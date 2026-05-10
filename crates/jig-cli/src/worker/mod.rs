//! Worker — the single abstraction for a Claude Code session.
//!
//! A Worker owns its identity and a [`WorktreeRef`] pointing at its
//! git worktree on disk.  The full [`Worktree`] (wrapping a git2 repo
//! handle) is resolved on demand — we never serialize what we can derive.

pub mod events;
mod status;

pub use status::{MuxStatus, WorkerStatus};

use std::path::Path;

use uuid::Uuid;

use events::{Event, EventKind, TerminalKind, WorkerState};
use jig_core::agents::Agent;
use jig_core::git::{Branch, Repo, Worktree, WorktreeRef};

use jig_core::issues::issue::IssueRef;
use jig_core::mux::Mux;
use jig_core::prompt::Prompt;


#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("worker '{0}' not found")]
    NotFound(String),

    #[error("worker '{0}' is still initializing")]
    Initializing(String),

    #[error("worker '{branch}' failed during setup: {reason}")]
    SetupFailed { branch: String, reason: String },

    #[error(transparent)]
    Mux(#[from] jig_core::mux::MuxError),

    #[error(transparent)]
    Git(#[from] jig_core::git::GitError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Agent(#[from] jig_core::agents::AgentError),

    #[error(transparent)]
    Template(#[from] handlebars::RenderError),

    #[error(transparent)]
    EventLog(#[from] jig_core::events::EventLogError),
}

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

    pub fn worktree(&self) -> Result<Worktree, WorkerError> {
        Ok(self.path.open()?)
    }

    pub fn log(&self) -> Result<events::EventLog, WorkerError> {
        let repo_name = self.repo_name();
        let log = events::event_log_for_worker(&repo_name, &self.branch)?;
        Ok(log)
    }

    pub fn status(&self) -> Option<WorkerStatus> {
        let log = self.log().ok()?;
        if !log.exists() {
            return None;
        }
        let state: WorkerState = log.reduce().ok()?;
        Some(state.status)
    }

    pub fn fail_reason(&self) -> Option<String> {
        let log = self.log().ok()?;
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

    pub fn remove(&self, force: bool) -> Result<(), WorkerError> {
        Ok(self.worktree()?.remove(force)?)
    }

    pub fn unregister(&self) -> Result<(), WorkerError> {
        if let Ok(log) = self.log() {
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
        let path: &Path = &self.path;
        for ancestor in path.ancestors() {
            if ancestor.file_name().map(|n| n == crate::context::JIG_DIR).unwrap_or(false) {
                if let Some(root) = ancestor.parent() {
                    return root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                }
            }
        }
        "unknown".to_string()
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
        prompt: Prompt,
        auto: bool,
        issue_ref: Option<IssueRef>,
        copy_files: &[std::path::PathBuf],
        on_create: Option<std::process::Command>,
        mux: &dyn Mux,
    ) -> Result<Self, WorkerError> {
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

        mux.create_window(&worker.branch, &worker.path)?;
        let cmd = agent.spawn(prompt)?;
        mux.send_keys(&worker.branch, &[&cmd, "Enter"])?;

        Ok(worker)
    }

    pub fn resume(wt: &Worktree, agent: &Agent, prompt: Prompt, mux: &dyn Mux) -> Result<Self, WorkerError> {
        let worker = Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref: None,
        };

        if let Ok(event_log) = worker.log() {
            let _ = event_log.append(&Event::now(EventKind::Resume));
        }

        mux.create_window(&worker.branch, &worker.path)?;
        let cmd = agent.resume(prompt)?;
        mux.send_keys(&worker.branch, &[&cmd, "Enter"])?;

        Ok(worker)
    }

    pub fn nudge(&self, prompt: Prompt, mux: &dyn Mux) -> Result<(), WorkerError> {
        let nudge_type_key = prompt.name().to_string();
        let message = prompt.render()?;

        mux.send_message(&self.branch, &message)?;

        if let Ok(event_log) = self.log() {
            let _ = event_log.append(&Event::now(EventKind::Nudge {
                nudge_type: nudge_type_key,
                message: message.clone(),
            }));
        }

        Ok(())
    }

    pub fn kill(&self, mux: &dyn Mux) -> Result<(), WorkerError> {
        mux.kill_window(&self.branch)?;
        Ok(())
    }

    pub fn attach(&self, mux: &dyn Mux) -> Result<(), WorkerError> {
        if !mux.window_exists(&self.branch) {
            if self.path.exists() {
                if let Some(status) = self.status() {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(WorkerError::Initializing(
                                self.branch.to_string(),
                            ));
                        }
                        WorkerStatus::Failed => {
                            let reason = self.fail_reason().unwrap_or_else(|| "unknown".into());
                            return Err(WorkerError::SetupFailed {
                                branch: self.branch.to_string(),
                                reason,
                            });
                        }
                        _ => {}
                    }
                }
            }
            return Err(WorkerError::NotFound(
                self.branch.to_string(),
            ));
        }
        mux.attach_window(&self.branch)?;
        Ok(())
    }

    pub fn is_orphaned(&self, mux: &dyn Mux) -> bool {
        if self.has_mux_window(mux) {
            return false;
        }
        match self.status() {
            Some(s) => {
                !s.is_terminal() && s != WorkerStatus::Initializing && s != WorkerStatus::Created
            }
            None => false,
        }
    }
}
