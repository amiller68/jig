//! Hooks command — manage hook integrations

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::op::{NoOutput, Op, OpContext};

/// Manage hook integrations
#[derive(Args, Debug, Clone)]
pub struct Hooks {
    #[command(subcommand)]
    pub subcommand: HooksCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommands {
    /// Install Claude Code hooks to ~/.claude/hooks/
    InstallClaude,
}

#[derive(Debug, thiserror::Error)]
pub enum HooksError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Hooks {
    type Error = HooksError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        match &self.subcommand {
            HooksCommands::InstallClaude => {
                let result = jig_core::hooks::install_claude_hooks()?;

                for name in &result.installed {
                    eprintln!("{} Installed {}", "✓".green(), name);
                }
                for name in &result.skipped {
                    eprintln!("{} Skipped {} (already exists)", "!".yellow(), name);
                }

                if result.installed.is_empty() && !result.skipped.is_empty() {
                    eprintln!("All hooks already installed.");
                }

                Ok(NoOutput)
            }
        }
    }
}
