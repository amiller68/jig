//! Issues command — discover, browse, and manage issues.

mod complete;
mod create;
pub mod list;
mod stats;
mod status;
mod update;

use std::fmt;
use std::io;

use clap::{Args, Subcommand};

use jig_core::issues::Issue as CoreIssue;

use crate::cli::op::Op;
use crate::cli::ui;

pub use list::List;

/// Discover and manage issues
#[derive(Args, Debug, Clone)]
pub struct Issues {
    #[command(subcommand)]
    pub command: Option<IssuesCommand>,

    #[command(flatten)]
    pub list: List,
}

#[derive(Subcommand, Debug, Clone)]
pub enum IssuesCommand {
    /// List and browse issues (default)
    List(List),
    /// Create a new issue
    Create(create::Create),
    /// Update an existing issue's fields
    Update(update::Update),
    /// Update issue status
    Status(status::Status),
    /// Mark an issue as complete
    Complete(complete::Complete),
    /// Show issue statistics
    Stats(stats::Stats),
}

#[derive(Debug, thiserror::Error)]
pub enum IssuesError {
    #[error(transparent)]
    Context(#[from] crate::context::ContextError),
    #[error(transparent)]
    Linear(#[from] jig_core::issues::providers::linear::client::LinearError),
    #[error("{0}")]
    Usage(String),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub enum IssuesOutput {
    Table(Vec<CoreIssue>, Option<Vec<String>>),
    Detail(Box<CoreIssue>),
    Interactive,
    Ids(Vec<String>),
    Created(String),
    Updated(String),
    StatusUpdated(String, String),
    Completed(String, bool),
    Stats(StatsData),
}

#[derive(Debug)]
pub struct StatsData {
    pub by_status: Vec<(String, usize)>,
    pub by_priority: Vec<(String, usize)>,
}

impl Op for Issues {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        match &self.command {
            Some(IssuesCommand::List(cmd)) => cmd.run(),
            Some(IssuesCommand::Create(cmd)) => cmd.run(),
            Some(IssuesCommand::Update(cmd)) => cmd.run(),
            Some(IssuesCommand::Status(cmd)) => cmd.run(),
            Some(IssuesCommand::Complete(cmd)) => cmd.run(),
            Some(IssuesCommand::Stats(cmd)) => cmd.run(),
            None => self.list.run(),
        }
    }
}

impl fmt::Display for IssuesOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Table(issues, auto_spawn_labels) => {
                if issues.is_empty() {
                    return write!(f, "No issues found");
                }
                if ui::is_plain() {
                    for issue in issues {
                        writeln!(
                            f,
                            "{}\t{}\t{}",
                            issue.status(),
                            issue.priority(),
                            issue.title()
                        )?;
                    }
                    return Ok(());
                }
                let table = list::render_table(issues, auto_spawn_labels.as_deref());
                write!(f, "{table}")
            }
            Self::Detail(issue) => {
                if let Some(parent) = &issue.parent() {
                    writeln!(f, "Parent: {}", parent)?;
                    writeln!(f)?;
                }
                write!(f, "{}", issue.body())?;
                if !issue.labels().is_empty() {
                    write!(f, "\n\nLabels: {}", issue.labels().join(", "))?;
                }
                if !issue.depends_on().is_empty() {
                    let deps: Vec<&str> = issue.depends_on().iter().map(|d| d.as_ref()).collect();
                    write!(f, "\n\nBlocked by: {}", deps.join(", "))?;
                }
                Ok(())
            }
            Self::Interactive => Ok(()),
            Self::Ids(ids) => {
                for id in ids {
                    writeln!(f, "{}", id)?;
                }
                Ok(())
            }
            Self::Created(id) => write!(f, "Created issue: {}", id),
            Self::Updated(id) => write!(f, "Updated issue: {}", id),
            Self::StatusUpdated(id, status) => write!(f, "Updated {} -> {}", id, status),
            Self::Completed(id, deleted) => {
                if *deleted {
                    write!(f, "Completed and deleted: {}", id)
                } else {
                    write!(f, "Completed: {}", id)
                }
            }
            Self::Stats(data) => {
                write!(f, "By Status:  ")?;
                for (i, (name, count)) in data.by_status.iter().enumerate() {
                    if i > 0 {
                        write!(f, "  ")?;
                    }
                    write!(f, "{}: {}", name, count)?;
                }
                writeln!(f)?;
                write!(f, "By Priority:")?;
                for (name, count) in &data.by_priority {
                    write!(f, "  {}: {}", name, count)?;
                }
                Ok(())
            }
        }
    }
}
