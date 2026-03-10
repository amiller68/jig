//! Config command

use clap::{Args, Subcommand};

use jig_core::config::{self, ConfigDisplay};
use jig_core::Error as CoreError;

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Manage configuration
#[derive(Args, Debug, Clone)]
pub struct Config {
    #[command(subcommand)]
    pub subcommand: Option<ConfigCommands>,

    /// List all configuration
    #[arg(long)]
    pub list: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Get or set base branch
    Base {
        /// Branch name to set
        branch: Option<String>,

        /// Use global default
        #[arg(long, short)]
        global: bool,

        /// Remove the setting
        #[arg(long)]
        unset: bool,
    },

    /// Get or set on-create hook
    OnCreate {
        /// Command to run
        command: Option<String>,

        /// Remove the hook
        #[arg(long)]
        unset: bool,
    },

    /// Show current configuration (default)
    Show,
}

/// Output for config commands (may output to stdout for get operations)
#[derive(Debug)]
pub struct ConfigOutput(Option<String>);

impl std::fmt::Display for ConfigOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref value) = self.0 {
            write!(f, "{}", value)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl Op for Config {
    type Error = ConfigError;
    type Output = ConfigOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        if self.list {
            return show_list();
        }

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => show_config(ctx),
            Some(ConfigCommands::Base {
                branch,
                global,
                unset,
            }) => handle_base(ctx, branch.as_deref(), *global, *unset),
            Some(ConfigCommands::OnCreate { command, unset }) => {
                handle_on_create(ctx, command.as_deref(), *unset)
            }
        }
    }
}

fn show_config(ctx: &RepoCtx) -> Result<ConfigOutput, ConfigError> {
    let repo = ctx.repo()?;
    let display = ConfigDisplay::load(&repo.repo_root)?;

    ui::header("Configuration");
    eprintln!();
    eprintln!(
        "  {} {}",
        ui::dim("Effective base branch:"),
        ui::highlight(&display.effective_base)
    );

    if let Some(ref toml) = display.toml_base {
        eprintln!("    {} {}", ui::dim("(jig.toml)"), toml);
    }
    if let Some(ref repo_base) = display.repo_base {
        eprintln!("    {} {}", ui::dim("(global config)"), repo_base);
    }
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", ui::dim("(global default)"), global);
    }

    if let Some(ref hook) = display.effective_on_create {
        eprintln!();
        eprintln!("  {} {}", ui::dim("On-create hook:"), ui::highlight(hook));
        if display.toml_on_create.is_some() {
            eprintln!("    {} jig.toml", ui::dim("(from)"));
        } else {
            eprintln!("    {} global config", ui::dim("(from)"));
        }
    }

    Ok(ConfigOutput(None))
}

fn show_list() -> Result<ConfigOutput, ConfigError> {
    let entries = config::list_all_config()?;

    if entries.is_empty() {
        eprintln!("No configuration set");
        return Ok(ConfigOutput(None));
    }

    for (category, key, value) in entries {
        eprintln!("{} {} = {}", ui::dim(&category), ui::highlight(&key), value);
    }

    Ok(ConfigOutput(None))
}

fn handle_base(
    ctx: &RepoCtx,
    branch: Option<&str>,
    global: bool,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    if unset {
        if global {
            config::unset_global_base_branch()?;
            ui::success("Unset global base branch");
        } else {
            let repo = ctx.repo()?;
            config::unset_repo_base_branch(&repo.repo_root)?;
            ui::success("Unset repo base branch");
        }
        return Ok(ConfigOutput(None));
    }

    match branch {
        Some(b) => {
            if global {
                config::set_global_base_branch(b)?;
                ui::success(&format!("Set global base branch to '{}'", ui::highlight(b)));
            } else {
                let repo = ctx.repo()?;
                config::set_repo_base_branch(&repo.repo_root, b)?;
                ui::success(&format!("Set repo base branch to '{}'", ui::highlight(b)));
            }
            Ok(ConfigOutput(None))
        }
        None => {
            // Get/show current value
            if global {
                match config::get_global_base_branch()? {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No global default set");
                        Ok(ConfigOutput(None))
                    }
                }
            } else {
                let repo = ctx.repo()?;
                match config::get_repo_base_branch(&repo.repo_root)? {
                    Some(b) => Ok(ConfigOutput(Some(b))),
                    None => {
                        eprintln!("No config set for this repo");
                        Ok(ConfigOutput(None))
                    }
                }
            }
        }
    }
}

fn handle_on_create(
    ctx: &RepoCtx,
    command: Option<&str>,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    let repo = ctx.repo()?;

    if unset {
        config::unset_on_create_hook(&repo.repo_root)?;
        ui::success("Unset on-create hook");
        return Ok(ConfigOutput(None));
    }

    match command {
        Some(cmd) => {
            config::set_on_create_hook(&repo.repo_root, cmd)?;
            ui::success(&format!("Set on-create hook to '{}'", ui::highlight(cmd)));
            Ok(ConfigOutput(None))
        }
        None => match config::get_on_create_hook(&repo.repo_root)? {
            Some(cmd) => Ok(ConfigOutput(Some(cmd))),
            None => {
                eprintln!("No on-create hook set");
                Ok(ConfigOutput(None))
            }
        },
    }
}
