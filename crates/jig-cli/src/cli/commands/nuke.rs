//! Nuke command — kill all workers, remove worktrees, clear state

use crate::context::{Context, RepoConfig};
use jig_core::git::Repo;
use jig_core::mux::{Mux, TmuxMux};

use crate::cli::op::{NoOutput, Op};
use crate::cli::ui;

/// Nuke all workers and state for this repo (keeps config)
#[derive(clap::Args, Debug, Clone)]
pub struct Nuke {
    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    global: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum NukeError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error(transparent)]
    Mux(#[from] jig_core::MuxError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Nuke {
    type Error = NukeError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.global {
            let cfg = Context::from_global()?;
            if cfg.repos.is_empty() {
                return Err(jig_core::Error::NotInGitRepo.into());
            }
            for repo in &cfg.repos {
                nuke_repo(repo)?;
            }
        } else {
            let cfg = Context::from_cwd()?;
            let repo = cfg.repo()?;
            nuke_repo(repo)?;
        }

        if let Ok(logs_dir) = crate::context::daemon_logs_dir() {
            if logs_dir.exists() {
                let _ = std::fs::remove_dir_all(&logs_dir);
                let _ = std::fs::create_dir_all(&logs_dir);
                ui::success("Cleared daemon logs");
            }
        }

        eprintln!();
        ui::success(&ui::bold("Nuked. Config and hooks are untouched."));

        Ok(NoOutput)
    }
}

fn nuke_repo(cfg: &RepoConfig) -> Result<(), NukeError> {
    let repo_name = cfg
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // 1. Kill mux session for this repo (takes out all windows at once)
    let session_name = cfg.session_name();
    let mux = TmuxMux::new(&session_name);
    mux.kill_all()?;
    ui::success(&format!(
        "Killed session '{}'",
        ui::highlight(&session_name)
    ));

    // 2. Clean up ALL event dirs for this repo (prefix match)
    if let Ok(events_dir) = crate::context::global_state_dir().map(|d| d.join("events")) {
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
        ui::success(&format!(
            "Removed worktree '{}'",
            ui::highlight(&wt.branch_name())
        ));
    }

    // 4. Clear event logs for this repo's workers
    if let Ok(events_dir) = crate::context::global_events_dir() {
        let prefix = format!("{}-", repo_name);
        if let Ok(entries) = std::fs::read_dir(&events_dir) {
            let mut cleared = 0;
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = std::fs::remove_dir_all(entry.path());
                    cleared += 1;
                }
            }
            if cleared > 0 {
                ui::success(&format!("Cleared {} worker event logs", cleared));
            }
        }
    }

    Ok(())
}
