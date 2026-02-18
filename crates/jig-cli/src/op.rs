//! Op trait — typed command pattern for CLI operations
//!
//! Every CLI command implements `Op`: it does work and returns typed data or
//! a typed error. Ops never print, never color, never touch the terminal.
//! Formatting lives in `Display` impls on the output types.

pub trait Op {
    type Error: std::error::Error + Send + Sync + 'static;
    type Output: std::fmt::Display;

    fn execute(&self) -> Result<Self::Output, Self::Error>;
}
