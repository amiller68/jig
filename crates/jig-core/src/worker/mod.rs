//! Worker — the single abstraction for a Claude Code session.
//!
//! A Worker owns its identity and a [`WorktreeRef`] pointing at its
//! git worktree on disk.  The full [`Worktree`] (wrapping a git2 repo
//! handle) is resolved on demand — we never serialize what we can derive.

pub mod events;
mod status;

pub use status::{TmuxStatus, WorkerStatus};

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agents::Agent;
use crate::error::Result;
use crate::git::{Branch, Repo, Worktree, WorktreeRef};
use crate::host::tmux::TmuxWindow;
use crate::issues::issue::IssueRef;
use crate::prompt::Prompt;
use events::{Event, EventKind, EventLog, TerminalKind};

// TODO (draft):this shouldnot be here
pub const SPAWN_PREAMBLE: &str = r#"AUTONOMOUS MODE: You have been spawned by jig as a parallel worker in auto mode (--dangerously-skip-permissions). Work independently without human interaction.

YOUR GOAL: Complete the task below and create a draft PR. Definition of done: code committed (conventional commits), draft PR created via `jig pr` or /draft, and issue marked complete (see completion instructions in the task). Call /review when ready.

IMPORTANT: Create the draft PR using `jig pr` (or `/draft`, which wraps it). NEVER use `gh pr create` directly — it bypasses parent branch resolution and will target the wrong base branch.

HOW MONITORING WORKS: A daemon watches your activity via tool-use events. If you go idle or get stuck for ~5 minutes, you'll receive automated nudge messages (up to {{max_nudges}}). After that, a human is notified. Do not wait for input.

IF YOU GET STUCK:
- Do NOT enter plan mode or ask for confirmation — just proceed
- If a command fails, try to fix it yourself
- If tests fail, debug and fix them
- If unsure about an approach, pick the simplest one and go
- If truly blocked, explain what's blocking you so the nudge system can relay it

AUTOMATED REVIEW: After you create a draft PR, an automated review agent may review your code. If it requests changes, you'll receive a nudge with the path to a review file (e.g. .jig/reviews/001.md). When that happens:

1. Read the review file to see the findings
2. Address each finding — fix issues or prepare explanations
3. Submit your response: jig review respond --review <N> (pipe your response markdown to stdin)
4. Commit and push your changes
5. The next review cycle triggers automatically on push

Response format (pipe to jig review respond --review N):

# Response to Review NNN

## Addressed
- `file:line` — finding description: what you did to fix it

## Disputed
- `file:line` — finding description: why you disagree

## Deferred
- `file:line` — finding description: why this is out of scope

## Notes
Any additional context.

TASK:
{{task_context}}
"#;

/// A Worker is a Claude Code session in an isolated git worktree.
///
/// Only identity, worktree path, and issue ref are persisted.
/// Everything else is derived at runtime via the [`Worktree`] handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub(crate) id: Uuid,
    pub(crate) branch: Branch,
    pub(crate) path: WorktreeRef,
    pub(crate) issue_ref: Option<IssueRef>,

    #[serde(skip)]
    pub(crate) auto_spawned: bool,
}

impl From<&Worktree> for Worker {
    fn from(wt: &Worktree) -> Self {
        Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref: None,
            auto_spawned: false,
        }
    }
}

impl Worker {
    /// Construct a worker handle from a repo root path and branch name.
    pub fn from_branch(repo_root: &Path, branch: Branch) -> Self {
        let worktree_path = repo_root.join(crate::config::JIG_DIR).join(&*branch);
        Self {
            id: Uuid::nil(),
            branch,
            path: WorktreeRef::new(worktree_path),
            issue_ref: None,
            auto_spawned: false,
        }
    }

    // ── Accessors ──

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

    pub fn worktree(&self) -> Result<Worktree> {
        Ok(self.path.open()?)
    }

    // ── Tmux ──

    pub fn tmux_window(&self) -> Result<TmuxWindow> {
        let wt = self.path.open()?;
        let session = format!("jig-{}", wt.repo_name());
        Ok(TmuxWindow::new(session, &*self.branch))
    }

    pub fn has_tmux_window(&self) -> bool {
        self.tmux_window().map(|w| w.exists()).unwrap_or(false)
    }

    pub fn is_agent_running(&self) -> bool {
        self.tmux_window().map(|w| w.is_running()).unwrap_or(false)
    }

    // ── Event log ──

    pub fn event_log(&self) -> Result<EventLog> {
        let repo_name = self.repo_name();
        let log = EventLog::for_worker(&repo_name, &self.branch)?;
        Ok(log)
    }

    // ── Agent lifecycle ──

    /// Create worktree, emit events, render prompt, start agent in tmux.
    ///
    /// Resolves copy_files, on_create_hook, and base branch from repo config.
    /// Emits `Initializing` before worktree creation. On failure, emits
    /// `Terminal(failed)`. On success, emits `Spawn` and starts the agent.
    pub fn spawn(
        repo: &Repo,
        branch: &Branch,
        base: &Branch,
        agent: &Agent,
        prompt: Prompt,
        auto: bool,
        issue_ref: Option<IssueRef>,
    ) -> Result<Self> {
        let repo_root = repo.clone_path();
        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let branch_name = branch.to_string();

        let event_log = EventLog::for_worker(&repo_name, &branch_name)?;
        event_log.reset()?;

        let _ = event_log.append(&Event::now(EventKind::Initializing {
            branch: branch_name.clone(),
            base: base.to_string(),
            auto,
        }));

        let wt = match Worktree::create(repo, branch, base) {
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
            auto_spawned: auto,
        };

        let context = prompt.render()?;
        let window = worker.tmux_window()?;
        window.create(&worker.path)?;
        let cmd = agent.spawn_command(&context);
        window.send_keys(&[&cmd, "Enter"])?;

        Ok(worker)
    }

    /// Resume an existing worker by relaunching its agent session.
    pub fn resume(wt: &Worktree, agent: &Agent, prompt: Prompt) -> Result<Self> {
        let worker = Self {
            id: Uuid::new_v4(),
            branch: wt.branch_name(),
            path: wt.as_ref(),
            issue_ref: None,
            auto_spawned: false,
        };
        let context = prompt.render()?;

        if let Ok(event_log) = worker.event_log() {
            let _ = event_log.append(&Event::now(EventKind::Resume));
        }

        let window = worker.tmux_window()?;
        window.create(&worker.path)?;
        let cmd = agent.resume_command(&context);
        window.send_keys(&[&cmd, "Enter"])?;

        Ok(worker)
    }

    /// Send a nudge to the worker's tmux window and emit event.
    pub fn nudge(&self, prompt: Prompt) -> Result<()> {
        let nudge_type_key = prompt.name().to_string();
        let message = prompt.render()?;

        let window = self.tmux_window()?;
        window.send_message(&message)?;

        if let Ok(event_log) = self.event_log() {
            let _ = event_log.append(&Event::now(EventKind::Nudge {
                nudge_type: nudge_type_key,
                message: message.clone(),
            }));
        }

        Ok(())
    }

    // ── Remove / Kill ──

    pub fn remove(&self, force: bool) -> Result<()> {
        Ok(self.worktree()?.remove(force)?)
    }

    pub fn kill(&self) -> Result<()> {
        let window = self.tmux_window()?;
        window.kill()?;
        Ok(())
    }

    pub fn unregister(&self) -> Result<()> {
        if let Ok(log) = self.event_log() {
            let _ = log.remove();
        }
        Ok(())
    }

    // ── Attach ──

    pub fn attach(&self) -> Result<()> {
        let window = self.tmux_window()?;
        if !window.exists() {
            if self.path.exists() {
                if let Some(status) = self.worker_status() {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(crate::error::Error::Custom(format!(
                                "worker '{}' is still initializing (running on-create hook)",
                                self.branch
                            )));
                        }
                        WorkerStatus::Failed => {
                            let reason = self.fail_reason().unwrap_or("unknown".into());
                            return Err(crate::error::Error::Custom(format!(
                                "worker '{}' failed during setup: {}",
                                self.branch, reason
                            )));
                        }
                        _ => {}
                    }
                }
            }
            return Err(crate::error::Error::Custom(format!(
                "worker '{}' not found",
                self.branch
            )));
        }
        window.attach()?;
        Ok(())
    }

    // ── Event-derived status ──

    pub fn worker_status(&self) -> Option<WorkerStatus> {
        let log = self.event_log().ok()?;
        let events = log.read_all().ok()?;
        if events.is_empty() {
            return None;
        }
        let health = crate::config::GlobalConfig::load()
            .map(|g| g.health)
            .unwrap_or_default();
        Some(events::WorkerState::reduce(&events, &health).status)
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

    // ── Tmux status ──

    pub fn tmux_status(&self) -> TmuxStatus {
        match self.tmux_window() {
            Ok(w) => {
                if !w.exists() {
                    TmuxStatus::NoWindow
                } else if w.is_running() {
                    TmuxStatus::Running
                } else {
                    TmuxStatus::Exited
                }
            }
            Err(_) => TmuxStatus::NoWindow,
        }
    }

    // ── Recovery ──

    /// Whether this worker is orphaned: has an active event log status but no tmux window.
    pub fn is_orphaned(&self) -> bool {
        if self.has_tmux_window() {
            return false;
        }
        match self.worker_status() {
            Some(s) => !s.is_terminal() && s != WorkerStatus::Initializing && s != WorkerStatus::Created,
            None => false,
        }
    }

    // ── Discovery ──

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

    fn repo_name(&self) -> String {
        self.worktree()
            .map(|wt| wt.repo_name())
            .unwrap_or_else(|_| "unknown".to_string())
    }
}
