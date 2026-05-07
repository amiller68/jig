//! Resume command — relaunch a dead worker's agent session

use clap::Args;

use crate::context;
use crate::worker::Worker;
use jig_core::agents;
use jig_core::mux::TmuxMux;
use jig_core::{Error, Worktree};

use crate::cli::op::{NoOutput, Op};
use crate::context::RepoConfig;
use crate::cli::ui;

/// Resume a dead worker by relaunching its agent session
#[derive(Args, Debug, Clone)]
pub struct Resume {
    /// Worker name to resume
    pub name: String,

    /// Override the task context for the resumed session
    #[arg(long, short)]
    pub context: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Resume {
    type Error = ResumeError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = RepoConfig::from_cwd()?;
        let repo_name = cfg.repo_root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mux = TmuxMux::for_repo(&repo_name);

        // Open existing worktree
        let wt_path = cfg.worktrees_path.join(&self.name);
        if !wt_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }
        let wt = Worktree::open(&wt_path)?;

        // Error if mux window already exists
        let pre = Worker::from(&wt);
        if pre.has_mux_window(&mux) {
            ui::failure(&format!(
                "Worker '{}' already has a window. Use '{}' to attach.",
                ui::highlight(&self.name),
                ui::highlight(&format!("jig attach {}", self.name))
            ));
            return Err(Error::Custom(format!(
                "Worker '{}' already running — use `jig attach` instead",
                self.name
            ))
            .into());
        }

        let effective_context = self
            .context
            .clone()
            .unwrap_or_else(|| "You were interrupted. Resume your previous task.".to_string());

        let jig_config = context::JigToml::load(&cfg.repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_config(
            &jig_config.agent.agent_type,
            Some(&jig_config.agent.model),
            &jig_config.agent.disallowed_tools,
        )
        .unwrap_or_else(|| agents::Agent::from_config("claude", None, &[]).unwrap());

        Worker::resume(&wt, &agent, &effective_context, &mux)?;

        ui::success(&format!(
            "Resumed worker '{}'",
            ui::highlight(&self.name)
        ));

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", self.name))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
