//! Version command - show version information

use std::convert::Infallible;

use clap::Args;

use crate::cli::op::{NoOutput, Op, RepoCtx};
use crate::cli::ui;

/// Show version information
#[derive(Args, Debug, Clone)]
pub struct Version;

impl Op for Version {
    type Error = Infallible;
    type Output = NoOutput;

    fn run(&self, _ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        eprintln!(
            "{} {}",
            ui::bold("jig"),
            ui::highlight(env!("CARGO_PKG_VERSION"))
        );
        Ok(NoOutput)
    }
}
