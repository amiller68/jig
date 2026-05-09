//! Shell setup command - automatically configures shell integration

use std::fs;
use std::path::PathBuf;

use clap::Args;

use crate::cli::op::{NoOutput, Op};
use crate::terminal::shell::{self, Shell, ShellError};

/// Automatically configure shell integration
#[derive(Args, Debug, Clone)]
pub struct ShellSetup {
    /// Show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ShellSetupError {
    #[error(transparent)]
    Shell(#[from] ShellError),
    #[error("Could not find home directory")]
    NoHomeDir,
    #[error("Failed to read {0}: {1}")]
    ReadFailed(PathBuf, std::io::Error),
    #[error("Failed to write {0}: {1}")]
    WriteFailed(PathBuf, std::io::Error),
    #[error("Failed to create directory {0}: {1}")]
    CreateDirFailed(PathBuf, std::io::Error),
}

impl Op for ShellSetup {
    type Error = ShellSetupError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let detected = Shell::detect()?;
        let home = dirs::home_dir().ok_or(ShellSetupError::NoHomeDir)?;
        let config_path = detected.config_file(&home);
        let (marker_start, marker_end) = shell::markers();

        println!("Detected shell: {}", detected.name());
        println!("Config file: {}", config_path.display());

        let existing_content = if config_path.exists() {
            fs::read_to_string(&config_path)
                .map_err(|e| ShellSetupError::ReadFailed(config_path.clone(), e))?
        } else {
            String::new()
        };

        if shell::has_existing_integration(&existing_content) {
            if existing_content.contains(marker_start) {
                println!("\njig shell integration is already configured.");
                println!("To reconfigure, remove the block between:");
                println!("  {marker_start}");
                println!("  {marker_end}");
            } else {
                println!(
                    "\njig shell integration appears to be configured (found 'jig shell-init')."
                );
                println!(
                    "If it's not working, check that the eval line comes AFTER your PATH setup."
                );
            }
            return Ok(NoOutput);
        }

        let integration_block = detected.integration_block();

        if self.dry_run {
            println!("\n[Dry run] Would add to {}:", config_path.display());
            println!("{}", integration_block.trim());
            return Ok(NoOutput);
        }

        if config_path.exists() && !existing_content.is_empty() {
            let backup_path = config_path.with_extension("bak");
            fs::write(&backup_path, &existing_content)
                .map_err(|e| ShellSetupError::WriteFailed(backup_path.clone(), e))?;
            println!("Created backup: {}", backup_path.display());
        }

        let new_content =
            if let Some(insert_after_line) = shell::find_last_path_line(&existing_content) {
                let lines: Vec<&str> = existing_content.lines().collect();
                let before: Vec<&str> = lines[..=insert_after_line].to_vec();
                let after: Vec<&str> = lines[insert_after_line + 1..].to_vec();

                let mut result = before.join("\n");
                result.push_str("\n\n");
                result.push_str(&integration_block);
                if !after.is_empty() {
                    result.push_str(&after.join("\n"));
                    result.push('\n');
                }
                result
            } else if existing_content.is_empty() {
                integration_block
            } else {
                format!("{}\n\n{}", existing_content.trim_end(), integration_block)
            };

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ShellSetupError::CreateDirFailed(parent.to_path_buf(), e))?;
        }

        fs::write(&config_path, &new_content)
            .map_err(|e| ShellSetupError::WriteFailed(config_path.clone(), e))?;

        println!("\nAdded jig shell integration to {}", config_path.display());
        println!("\nTo activate, either:");
        println!("  1. Open a new terminal, or");
        println!("  2. Run: source {}", config_path.display());
        println!("\nVerify with: type jig");
        println!("Expected output: \"jig is a shell function\" (or similar)");

        Ok(NoOutput)
    }
}
