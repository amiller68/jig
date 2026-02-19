//! Op trait — typed command pattern for CLI operations
//!
//! Every CLI command implements `Op`: it does work and returns typed data or
//! a typed error. Formatting lives in `Display` impls on the output types.

use std::error::Error;
use std::fmt::Display;

/// Context passed to all operations
#[derive(Clone, Default)]
pub struct OpContext {
    /// Open/cd into worktree after creating
    pub open: bool,
    /// Skip on-create hook execution
    pub no_hooks: bool,
}

impl OpContext {
    pub fn new(open: bool, no_hooks: bool) -> Self {
        Self { open, no_hooks }
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
