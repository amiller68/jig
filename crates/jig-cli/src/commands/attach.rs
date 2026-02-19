//! Attach command - attach to tmux session

use clap::Args;

use jig_core::spawn;

use crate::op::{NoOutput, Op, OpContext};

/// Attach to tmux session
#[derive(Args, Debug, Clone)]
pub struct Attach {
    /// Window name to switch to
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Attach {
    type Error = AttachError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        spawn::attach(self.name.as_deref())?;
        Ok(NoOutput)
    }
}
