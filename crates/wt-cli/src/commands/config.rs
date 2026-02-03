//! Config command

use anyhow::Result;
use colored::Colorize;

use crate::cli::ConfigCommands;
use wt_core::config::{self, ConfigDisplay};

pub fn run(subcommand: Option<ConfigCommands>, list: bool) -> Result<()> {
    if list {
        return show_list();
    }

    match subcommand {
        None | Some(ConfigCommands::Show) => show_config(),
        Some(ConfigCommands::Base { branch, global, unset }) => {
            handle_base(branch.as_deref(), global, unset)
        }
        Some(ConfigCommands::OnCreate { command, unset }) => {
            handle_on_create(command.as_deref(), unset)
        }
    }
}

fn show_config() -> Result<()> {
    let display = ConfigDisplay::load_auto()?;

    eprintln!("{}", "Configuration".bold());
    eprintln!();
    eprintln!(
        "  {} {}",
        "Effective base branch:".dimmed(),
        display.effective_base.cyan()
    );

    if let Some(ref repo) = display.repo_base {
        eprintln!("    {} {}", "(repo)".dimmed(), repo);
    }
    if let Some(ref global) = display.global_base {
        eprintln!("    {} {}", "(global)".dimmed(), global);
    }

    if let Some(ref hook) = display.on_create_hook {
        eprintln!();
        eprintln!("  {} {}", "On-create hook:".dimmed(), hook.cyan());
    }

    Ok(())
}

fn show_list() -> Result<()> {
    let entries = config::list_all_config()?;

    if entries.is_empty() {
        eprintln!("No configuration set");
        return Ok(());
    }

    for (category, key, value) in entries {
        eprintln!("{} {} = {}", category.dimmed(), key.cyan(), value);
    }

    Ok(())
}

fn handle_base(branch: Option<&str>, global: bool, unset: bool) -> Result<()> {
    if unset {
        if global {
            config::unset_global_base_branch()?;
            eprintln!("{} Unset global base branch", "✓".green());
        } else {
            config::unset_repo_base_branch()?;
            eprintln!("{} Unset repo base branch", "✓".green());
        }
        return Ok(());
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
        }
        None => {
            // Get/show current value
            if global {
                match config::get_global_base_branch()? {
                    Some(b) => println!("{}", b),
                    None => eprintln!("No global default set"),
                }
            } else {
                match config::get_repo_base_branch()? {
                    Some(b) => println!("{}", b),
                    None => eprintln!("No config set for this repo"),
                }
            }
        }
    }

    Ok(())
}

fn handle_on_create(command: Option<&str>, unset: bool) -> Result<()> {
    if unset {
        config::unset_on_create_hook()?;
        eprintln!("{} Unset on-create hook", "✓".green());
        return Ok(());
    }

    match command {
        Some(cmd) => {
            config::set_on_create_hook(cmd)?;
            eprintln!("{} Set on-create hook to '{}'", "✓".green(), cmd.cyan());
        }
        None => {
            match config::get_on_create_hook()? {
                Some(cmd) => println!("{}", cmd),
                None => eprintln!("No on-create hook set"),
            }
        }
    }

    Ok(())
}
