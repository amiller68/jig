//! Kill command - kill a running tmux window

use clap::Args;
use colored::Colorize;

use jig_core::spawn;

use crate::op::{NoOutput, Op, OpContext};

/// Kill a running tmux window
#[derive(Args, Debug, Clone)]
pub struct Kill {
    /// Worktree name
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum KillError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Kill {
    type Error = KillError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        // Kill tmux window
        spawn::kill_window(&self.name)?;

        // Unregister from spawn state
        spawn::unregister(&self.name)?;

        eprintln!("{} Killed session '{}'", "✓".green(), self.name.cyan());

        Ok(NoOutput)
    }
}
