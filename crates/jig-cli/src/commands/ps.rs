//! Ps command — show status of spawned sessions

use std::fmt;

use colored::Colorize;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};

use jig_core::spawn::{self, TaskInfo, TaskStatus};

use crate::op::Op;

/// Show status of spawned sessions.
pub struct Ps;

#[derive(Debug, thiserror::Error)]
pub enum PsError {
    #[error("failed to list tasks: {0}")]
    ListTasks(#[from] jig_core::Error),
}

pub struct PsOutput {
    pub tasks: Vec<TaskInfo>,
}

impl Op for Ps {
    type Error = PsError;
    type Output = PsOutput;

    fn execute(&self) -> Result<Self::Output, Self::Error> {
        let tasks = spawn::list_tasks()?;
        Ok(PsOutput { tasks })
    }
}

impl fmt::Display for PsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tasks.is_empty() {
            return write!(f, "No spawned sessions");
        }

        let mut table = Table::new();
        table
            .load_preset(presets::NOTHING)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("NAME").add_attribute(Attribute::Bold),
                Cell::new("STATUS").add_attribute(Attribute::Bold),
                Cell::new("BRANCH").add_attribute(Attribute::Bold),
                Cell::new("COMMITS").add_attribute(Attribute::Bold),
                Cell::new("DIRTY").add_attribute(Attribute::Bold),
            ]);

        for task in &self.tasks {
            let (status_text, status_color) = match task.status {
                TaskStatus::Running => (task.status.as_str(), Color::Green),
                TaskStatus::Exited => (task.status.as_str(), Color::Yellow),
                TaskStatus::NoSession | TaskStatus::NoWindow => (task.status.as_str(), Color::Red),
            };

            let dirty_indicator = if task.is_dirty {
                "●".yellow().to_string()
            } else {
                "-".dimmed().to_string()
            };

            table.add_row(vec![
                Cell::new(&task.name).fg(Color::Cyan),
                Cell::new(status_text).fg(status_color),
                Cell::new(&task.branch),
                Cell::new(task.commits_ahead).set_alignment(CellAlignment::Right),
                Cell::new(&dirty_indicator).set_alignment(CellAlignment::Center),
            ]);
        }

        write!(f, "{table}")
    }
}
