//! Which command - show path to jig executable

use clap::Args;
use std::path::PathBuf;

use crate::op::{Op, OpContext};

/// Show path to jig executable
#[derive(Args, Debug, Clone)]
pub struct Which;

#[derive(Debug)]
pub struct WhichOutput(PathBuf);

impl std::fmt::Display for WhichOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WhichError {
    #[error("Failed to get executable path: {0}")]
    IoError(#[from] std::io::Error),
}

impl Op for Which {
    type Error = WhichError;
    type Output = WhichOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let exe = std::env::current_exe()?;
        Ok(WhichOutput(exe))
    }
}
