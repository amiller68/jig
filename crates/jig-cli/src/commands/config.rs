//! Config command

use clap::{Args, Subcommand};
use colored::Colorize;

use jig_core::config::{self, ConfigDisplay};
use jig_core::Error as CoreError;

use crate::op::{Op, OpContext};

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

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        if self.list {
            return show_list();
        }

        match &self.subcommand {
            None | Some(ConfigCommands::Show) => show_config(),
            Some(ConfigCommands::Base {
                branch,
                global,
                unset,
            }) => handle_base(branch.as_deref(), *global, *unset),
            Some(ConfigCommands::OnCreate { command, unset }) => {
                handle_on_create(command.as_deref(), *unset)
            }
        }
    }
}

fn show_config() -> Result<ConfigOutput, ConfigError> {
    let display = ConfigDisplay::load_auto()?;

    eprintln!("{}", "Configuration".bold());
    eprintln!();
    eprintln!(
        "  {} {}",
        "Effective base branch:".dimmed(),
        display.effective_base.cyan()
    );

    if let Some(ref toml) = display.toml_base {
        eprintln!("    {} {}", "(jig.toml)".dimmed(), toml);
    }
    if let Some(ref repo) = display.repo_base {
        eprintln!("    {} {}", "(global config)".dimmed(), repo);
    }
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", "(global default)".dimmed(), global);
    }

    if let Some(ref hook) = display.effective_on_create {
        eprintln!();
        eprintln!("  {} {}", "On-create hook:".dimmed(), hook.cyan());
        if display.toml_on_create.is_some() {
            eprintln!("    {} jig.toml", "(from)".dimmed());
        } else {
            eprintln!("    {} global config", "(from)".dimmed());
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
        eprintln!("{} {} = {}", category.dimmed(), key.cyan(), value);
    }

    Ok(ConfigOutput(None))
}

fn handle_base(
    branch: Option<&str>,
    global: bool,
    unset: bool,
) -> Result<ConfigOutput, ConfigError> {
    if unset {
        if global {
            config::unset_global_base_branch()?;
            eprintln!("{} Unset global base branch", "✓".green());
        } else {
            config::unset_repo_base_branch()?;
            eprintln!("{} Unset repo base branch", "✓".green());
        }
        return Ok(ConfigOutput(None));
    }

    match branch {
        Some(b) => {
            if global {
                config::set_global_base_branch(b)?;
                eprintln!("{} Set global base branch to '{}'", "✓".green(), b.cyan());
            } else {
                config::set_repo_base_branch(b)?;
                eprintln!("{} Set repo base branch to '{}'", "✓".green(), b.cyan());
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
                match config::get_repo_base_branch()? {
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

fn handle_on_create(command: Option<&str>, unset: bool) -> Result<ConfigOutput, ConfigError> {
    if unset {
        config::unset_on_create_hook()?;
        eprintln!("{} Unset on-create hook", "✓".green());
        return Ok(ConfigOutput(None));
    }

    match command {
        Some(cmd) => {
            config::set_on_create_hook(cmd)?;
            eprintln!("{} Set on-create hook to '{}'", "✓".green(), cmd.cyan());
            Ok(ConfigOutput(None))
        }
        None => match config::get_on_create_hook()? {
            Some(cmd) => Ok(ConfigOutput(Some(cmd))),
            None => {
                eprintln!("No on-create hook set");
                Ok(ConfigOutput(None))
            }
        },
    }
}
