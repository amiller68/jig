//! Resume command — re-launch a dead worker in tmux

use clap::Args;
use colored::Colorize;

use jig_core::{spawn, Error};

use crate::op::{NoOutput, Op, RepoCtx};

/// Resume a worker whose tmux window has died
#[derive(Args, Debug, Clone)]
pub struct Resume {
    /// Worker name to resume
    pub name: String,

    /// Override the original task context
    #[arg(long, short)]
    pub context: Option<String>,

    /// Resume in auto mode (full autonomous prompt)
    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error(transparent)]
    Core(#[from] Error),
}

impl Op for Resume {
    type Error = ResumeError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        // Determine context: --context flag takes precedence, else read from event log
        let repo_name = repo
            .repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let effective_context = match &self.context {
            Some(ctx) => Some(ctx.clone()),
            None => spawn::extract_spawn_context(&repo_name, &self.name),
        };

        // Determine auto mode: --auto flag, or check original spawn
        let use_auto = if self.auto {
            true
        } else {
            spawn::was_auto_spawn(&repo_name, &self.name)
        };

        spawn::resume_worker(repo, &self.name, use_auto, effective_context.as_deref())?;

        eprintln!(
            "{} Resumed worker '{}' in tmux",
            "✓".green(),
            self.name.cyan()
        );

        if use_auto {
            eprintln!("  {} Auto mode enabled", "→".dimmed());
        }

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            format!("jig attach {}", self.name).cyan()
        );

        Ok(NoOutput)
    }
}
