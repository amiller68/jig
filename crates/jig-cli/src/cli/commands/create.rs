//! Create worktree command

use clap::Args;
use std::path::PathBuf;

use crate::worker::events::{self, Event, EventKind};
use jig_core::git::Branch;
use jig_core::{Error, Worktree};

use crate::cli::op::Op;
use crate::context::Context;
use crate::cli::ui;

/// Create a new worktree
#[derive(Args, Debug, Clone)]
pub struct Create {
    /// Worktree name
    pub name: String,

    /// Branch name (defaults to worktree name)
    pub branch: Option<String>,

    /// Open/cd into worktree after creating
    #[arg(short = 'o')]
    pub open: bool,

    /// Base branch to create worktree from (overrides jig.toml default)
    #[arg(long, short = 'b')]
    pub base: Option<String>,

    /// Skip on-create hook execution
    #[arg(long = "no-hooks")]
    pub no_hooks: bool,
}

/// Output containing optional cd command
#[derive(Debug)]
pub enum CreateOutput {
    /// No output (created without -o flag)
    None,
    /// cd command to stdout
    Cd(PathBuf),
}

impl std::fmt::Display for CreateOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateOutput::None => Ok(()),
            CreateOutput::Cd(path) => write!(f, "cd '{}'", path.display()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Create {
    type Error = CreateError;
    type Output = CreateOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;

        let base_branch = match &self.base {
            Some(b) => Branch::new(b),
            None => repo.base_branch(&cfg.config),
        };

        let git_repo = jig_core::Repo::open(&repo.repo_root)?;
        let branch = Branch::new(self.branch.as_deref().unwrap_or(&self.name));
        let copy_files: Vec<std::path::PathBuf> =
            repo.repo.worktree.copy.iter().map(std::path::PathBuf::from).collect();
        let on_create = repo.repo.worktree.on_create.as_ref().map(|cmd| {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", cmd]);
            c
        });
        let wt = Worktree::create(&git_repo, &branch, &base_branch, &copy_files, on_create)?;

        // Emit Create event so the daemon knows this is a bare worktree
        let repo_name = repo
            .repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Ok(event_log) = events::event_log_for_worker(&repo_name, &self.name) {
            if let Err(e) = event_log.append(&Event::now(EventKind::Create {
                branch: branch.to_string(),
            })) {
                tracing::warn!(worker = %self.name, error = %e, "failed to emit Create event");
            }
        }

        ui::success(&format!(
            "Created worktree '{}' on branch '{}'",
            ui::highlight(&self.name),
            ui::highlight(&branch)
        ));

        // Output cd command if -o flag
        if self.open {
            let canonical = wt.path().canonicalize()?;
            Ok(CreateOutput::Cd(canonical))
        } else {
            Ok(CreateOutput::None)
        }
    }
}
