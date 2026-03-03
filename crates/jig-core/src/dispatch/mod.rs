//! Action dispatch system.
//!
//! Compares old and new worker states, produces actions to execute.

mod actions;
mod rules;

pub use actions::Action;
pub use rules::dispatch_actions;
