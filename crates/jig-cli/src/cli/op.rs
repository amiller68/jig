//! Op trait — typed command pattern for CLI operations
//!
//! Every CLI command implements `Op`: it does work and returns typed data or
//! a typed error. Formatting lives in `Display` impls on the output types.

use std::error::Error;
use std::fmt::Display;

/// Single-repo context (no -g). Current repo if available.
pub struct RepoCtx {
    pub config: Option<crate::config::Config>,
}

impl RepoCtx {
    /// Get the config, or error if not in a git repo.
    pub fn config(&self) -> std::result::Result<&crate::config::Config, jig_core::Error> {
        self.config.as_ref().ok_or(jig_core::Error::NotInGitRepo)
    }
}

/// Global context (-g). All registered repos.
pub struct GlobalCtx {
    pub configs: Vec<crate::config::Config>,
}

impl GlobalCtx {
    /// Find the repo that contains a worktree with the given name.
    pub fn config_for_worktree(
        &self,
        name: &str,
    ) -> std::result::Result<&crate::config::Config, jig_core::Error> {
        for cfg in &self.configs {
            let path = cfg.worktrees_path.join(name);
            if path.exists() {
                return Ok(cfg);
            }
        }
        Err(jig_core::Error::WorktreeNotFound(name.to_string()))
    }
}

/// Trait for CLI operations.
pub trait Op {
    type Error: Error + Send + Sync + 'static;
    type Output: Display;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error>;

    fn run_global(&self, _ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        eprintln!("error: this command does not support -g/--global");
        std::process::exit(1);
    }
}

/// Unit output for commands that only produce stderr
#[derive(Debug, Default)]
pub struct NoOutput;

impl Display for NoOutput {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// Macro to generate Command enum with Op implementation
#[macro_export]
macro_rules! command_enum {
    ($($(#[$attr:meta])* ($variant:ident, $type:ty)),* $(,)?) => {
        #[derive(clap::Subcommand, Debug, Clone)]
        #[allow(clippy::large_enum_variant)]
        pub enum Command {
            $(
                $(#[$attr])*
                $variant($type),
            )*
        }

        #[derive(Debug)]
        #[allow(clippy::large_enum_variant)]
        pub enum OpOutput {
            $($variant(<$type as $crate::cli::op::Op>::Output),)*
        }

        #[derive(Debug, thiserror::Error)]
        pub enum OpError {
            $(
                #[error(transparent)]
                $variant(<$type as $crate::cli::op::Op>::Error),
            )*
        }

        impl $crate::cli::op::Op for Command {
            type Output = OpOutput;
            type Error = OpError;

            fn run(&self, ctx: &$crate::cli::op::RepoCtx) -> Result<Self::Output, Self::Error> {
                match self {
                    $(
                        Command::$variant(op) => {
                            op.run(ctx)
                                .map(OpOutput::$variant)
                                .map_err(OpError::$variant)
                        },
                    )*
                }
            }

            fn run_global(&self, ctx: &$crate::cli::op::GlobalCtx) -> Result<Self::Output, Self::Error> {
                match self {
                    $(
                        Command::$variant(op) => {
                            op.run_global(ctx)
                                .map(OpOutput::$variant)
                                .map_err(OpError::$variant)
                        },
                    )*
                }
            }
        }

        impl std::fmt::Display for OpOutput {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        OpOutput::$variant(output) => write!(f, "{}", output),
                    )*
                }
            }
        }
    };
}
