//! Template engine for context injection.
//!
//! Uses Handlebars to render prompts for spawning, resuming, and nudging workers.
//! Templates follow a hierarchy: repo-specific > user global > built-in.

mod builtin;
mod engine;

pub use engine::{TemplateContext, TemplateEngine};
