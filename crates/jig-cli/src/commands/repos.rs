//! Repos command — list tracked repositories

use std::fmt;

use clap::Args;
use colored::Colorize;
use comfy_table::{presets, Attribute, Cell, Color, ContentArrangement, Table};

use jig_core::RepoRegistry;

use crate::op::{Op, RepoCtx};

/// List tracked repositories
#[derive(Args, Debug, Clone)]
pub struct Repos {}

#[derive(Debug, thiserror::Error)]
pub enum ReposError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

#[derive(Debug)]
pub enum ReposOutput {
    List(Vec<ReposListEntry>),
}

#[derive(Debug)]
pub struct ReposListEntry {
    pub name: String,
    pub path: String,
    pub last_used: String,
}

impl Op for Repos {
    type Error = ReposError;
    type Output = ReposOutput;

    fn run(&self, _ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let registry = RepoRegistry::load()?;
        let entries: Vec<ReposListEntry> = registry
            .repos()
            .iter()
            .map(|e| {
                let path = e.path.display().to_string();
                let name = e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                ReposListEntry {
                    name,
                    path,
                    last_used: e.last_used.format("%Y-%m-%d %H:%M").to_string(),
                }
            })
            .collect();
        Ok(ReposOutput::List(entries))
    }
}

impl fmt::Display for ReposOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReposOutput::List(entries) => {
                if entries.is_empty() {
                    write!(f, "{}", "No tracked repositories".dimmed())?;
                } else {
                    let mut table = Table::new();
                    table
                        .load_preset(presets::NOTHING)
                        .set_content_arrangement(ContentArrangement::Dynamic)
                        .set_header(vec![
                            Cell::new("NAME").add_attribute(Attribute::Bold),
                            Cell::new("PATH").add_attribute(Attribute::Bold),
                            Cell::new("LAST USED").add_attribute(Attribute::Bold),
                        ]);

                    for entry in entries {
                        table.add_row(vec![
                            Cell::new(&entry.name).fg(Color::Cyan),
                            Cell::new(&entry.path),
                            Cell::new(&entry.last_used),
                        ]);
                    }

                    write!(f, "{table}")?;
                }
            }
        }
        Ok(())
    }
}
