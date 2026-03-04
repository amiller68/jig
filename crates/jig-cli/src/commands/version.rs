//! Version command - show version information

use std::convert::Infallible;

use clap::Args;
use colored::Colorize;

use crate::op::{NoOutput, Op, RepoCtx};

/// Show version information
#[derive(Args, Debug, Clone)]
pub struct Version;

impl Op for Version {
    type Error = Infallible;
    type Output = NoOutput;

    fn run(&self, _ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        eprintln!("{} {}", "jig".bold(), env!("CARGO_PKG_VERSION").cyan());
        Ok(NoOutput)
    }
}
