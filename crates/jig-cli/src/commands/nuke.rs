//! Nuke command — kill all workers, remove worktrees, clear state

use jig_core::git::Repo;
use jig_core::{global_state_dir, TmuxSession, WorkersState};

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};
use crate::ui;

/// Nuke all workers and state for this repo (keeps config)
#[derive(clap::Args, Debug, Clone)]
pub struct Nuke;

#[derive(Debug, thiserror::Error)]
pub enum NukeError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error(transparent)]
    Tmux(#[from] jig_core::host::tmux::TmuxError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Nuke {
    type Error = NukeError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;
        nuke_repo(cfg)?;

        eprintln!();
        ui::success(&ui::bold("Nuked. Config and hooks are untouched."));

        Ok(NoOutput)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if ctx.configs.is_empty() {
            return Err(jig_core::Error::NotInGitRepo.into());
        }

        for cfg in &ctx.configs {
            nuke_repo(cfg)?;
        }

        eprintln!();
        ui::success(&ui::bold("Nuked. Config and hooks are untouched."));

        Ok(NoOutput)
    }
}

fn nuke_repo(cfg: &jig_core::Config) -> Result<(), NukeError> {
    let repo_name = cfg
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // 1. Kill tmux session for this repo (takes out all windows at once)
    let session_name = cfg.session_name();
    let session = TmuxSession::new(&session_name);
    if session.exists() {
        session.kill()?;
        ui::success(&format!(
            "Killed tmux session '{}'",
            ui::highlight(&session_name)
        ));
    }

    // 2. Clean up ALL event dirs for this repo (prefix match)
    if let Ok(events_dir) = global_state_dir().map(|d| d.join("events")) {
        if let Ok(entries) = std::fs::read_dir(&events_dir) {
            let prefix = format!("{}-", repo_name);
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
        ui::success(&format!(
            "Cleared event logs for {}",
            ui::highlight(&repo_name)
        ));
    }

    // 3. Remove git worktrees
    let worktrees = Repo::open(&cfg.repo_root)
        .and_then(|r| r.list_worktrees())
        .unwrap_or_default();
    for wt in &worktrees {
        if wt.remove(true).is_err() {
            let _ = std::fs::remove_dir_all(wt.path());
        }
        ui::success(&format!("Removed worktree '{}'", ui::highlight(&wt.branch_name())));
    }

    // 4. Clear global worker entries for this repo
    if let Ok(mut global) = WorkersState::load() {
        let keys: Vec<_> = global
            .workers_for_repo(&repo_name)
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();
        if !keys.is_empty() {
            for key in &keys {
                global.remove_worker(key);
            }
            let _ = global.save();
            ui::success(&format!("Cleared {} global worker entries", keys.len()));
        }
    }

    Ok(())
}
