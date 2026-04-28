//! Resume command — relaunch a dead worker's agent session

use clap::Args;

use jig_core::agents;
use jig_core::{config, Error, Prompt, Worker, Worktree};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

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

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;

        // Open existing worktree
        let wt_path = cfg.worktrees_path.join(&self.name);
        if !wt_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }
        let wt = Worktree::open(&wt_path)?;

        // Error if tmux window already exists
        let pre = Worker::from(&wt);
        if pre.has_tmux_window() {
            ui::failure(&format!(
                "Worker '{}' already has a tmux window. Use '{}' to attach.",
                ui::highlight(&self.name),
                ui::highlight(&format!("jig attach {}", self.name))
            ));
            return Err(Error::Custom(format!(
                "Worker '{}' already running — use `jig attach` instead",
                self.name
            ))
            .into());
        }

        // Read original context from spawn event if no override provided
        let effective_context = if let Some(ref ctx_override) = self.context {
            ctx_override.clone()
        } else {
            let repo_name = cfg
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            jig_daemon::recovery::RecoveryScanner::read_spawn_context(&repo_name, &self.name)
                .unwrap_or_else(|| "You were interrupted. Resume your previous task.".to_string())
        };

        let jig_config = config::JigToml::load(&cfg.repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&jig_config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude))
            .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());

        let prompt = Prompt::new(jig_core::worker::SPAWN_PREAMBLE).task(&effective_context);

        Worker::resume(&wt, &agent, prompt)?;

        ui::success(&format!(
            "Resumed worker '{}' in tmux",
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
