//! Op trait — typed command pattern for CLI operations
//!
//! Every CLI command implements `Op`: it does work and returns typed data or
//! a typed error. Formatting lives in `Display` impls on the output types.

use std::error::Error;
use std::fmt::Display;

/// Context passed to all operations.
///
/// Contains the resolved list of repos to operate on:
/// - Without `-g`: just the current repo (if in one)
/// - With `-g`: all registered repos
pub struct OpContext {
    /// Whether `-g` was passed.
    pub global: bool,
    /// Resolved repos to operate on.
    pub repos: Vec<jig_core::RepoContext>,
}

impl OpContext {
    pub fn new(global: bool) -> Self {
        let repos = if global {
            let registry = jig_core::RepoRegistry::load().unwrap_or_default();
            registry
                .repos()
                .iter()
                .filter(|e| e.path.exists())
                .filter_map(|e| jig_core::RepoContext::from_path(&e.path).ok())
                .collect()
        } else {
            jig_core::RepoContext::from_cwd().ok().into_iter().collect()
        };
        Self { global, repos }
    }

    /// Get the single repo context, or error if not in a git repo.
    /// For commands that operate on one repo at a time.
    pub fn repo(&self) -> std::result::Result<&jig_core::RepoContext, jig_core::Error> {
        self.repos.first().ok_or(jig_core::Error::NotInGitRepo)
    }
}

/// Trait for CLI operations
pub trait Op {
    type Error: Error + Send + Sync + 'static;
    type Output: Display;

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error>;
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
///
/// Usage:
/// ```ignore
/// command_enum! {
///     #[command(visible_alias = "c")]
///     (Create, crate::commands::Create),
///     (List, crate::commands::List),
/// }
/// ```
#[macro_export]
macro_rules! command_enum {
    ($($(#[$attr:meta])* ($variant:ident, $type:ty)),* $(,)?) => {
        #[derive(clap::Subcommand, Debug, Clone)]
        pub enum Command {
            $(
                $(#[$attr])*
                $variant($type),
            )*
        }

        #[derive(Debug)]
        pub enum OpOutput {
            $($variant(<$type as $crate::op::Op>::Output),)*
        }

        #[derive(Debug, thiserror::Error)]
        pub enum OpError {
            $(
                #[error(transparent)]
                $variant(<$type as $crate::op::Op>::Error),
            )*
        }

        impl $crate::op::Op for Command {
            type Output = OpOutput;
            type Error = OpError;

            fn execute(&self, ctx: &$crate::op::OpContext) -> Result<Self::Output, Self::Error> {
                match self {
                    $(
                        Command::$variant(op) => {
                            op.execute(ctx)
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
