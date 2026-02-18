//! Update command - update jig to latest version

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use colored::Colorize;

const GITHUB_REPO: &str = "amiller68/jig";
const INSTALL_SCRIPT_URL: &str = "https://raw.githubusercontent.com/amiller68/jig/main/install.sh";

/// Installation method detection
#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallMethod {
    /// Installed via install script to ~/.local/bin
    Script(PathBuf),
    /// Installed via cargo install
    Cargo(PathBuf),
    /// Running from source/target directory
    Source(PathBuf),
    /// Unknown installation method
    Unknown(PathBuf),
}

impl InstallMethod {
    fn description(&self) -> &str {
        match self {
            InstallMethod::Script(_) => "install script (~/.local/bin)",
            InstallMethod::Cargo(_) => "cargo install (~/.cargo/bin)",
            InstallMethod::Source(_) => "source build (target/)",
            InstallMethod::Unknown(_) => "unknown",
        }
    }
}

/// Detect how jig was installed
fn detect_installation() -> Result<InstallMethod> {
    let exe_path = std::env::current_exe().context("Failed to get executable path")?;
    let path_str = exe_path.to_string_lossy();

    if path_str.contains("/.local/bin/") {
        Ok(InstallMethod::Script(exe_path))
    } else if path_str.contains("/.cargo/bin/") {
        Ok(InstallMethod::Cargo(exe_path))
    } else if path_str.contains("/target/") {
        Ok(InstallMethod::Source(exe_path))
    } else {
        Ok(InstallMethod::Unknown(exe_path))
    }
}

/// Fetch the latest version from GitHub releases
fn get_latest_version() -> Result<String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            &format!(
                "https://api.github.com/repos/{}/releases/latest",
                GITHUB_REPO
            ),
        ])
        .output()
        .context("Failed to run curl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to fetch latest version: {}", stderr);
    }

    let body = String::from_utf8_lossy(&output.stdout);

    // Parse tag_name from JSON response
    // Looking for: "tag_name": "v0.5.1",
    for line in body.lines() {
        if line.contains("\"tag_name\"") {
            if let Some(start) = line.find(':') {
                let value = &line[start + 1..];
                // Strip whitespace, trailing comma, then quotes
                let value = value.trim().trim_end_matches(',').trim_matches('"');
                // Remove leading 'v' if present
                let version = value.trim_start_matches('v');
                return Ok(version.to_string());
            }
        }
    }

    bail!("Could not parse version from GitHub response")
}

/// Prompt user for yes/no confirmation
fn prompt_confirm(message: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    eprint!("{} {} ", message, suffix);
    io::stderr().flush()?;

    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;

    let answer = line.trim().to_lowercase();
    if answer.is_empty() {
        Ok(default_yes)
    } else {
        Ok(answer == "y" || answer == "yes")
    }
}

/// Run the install script
fn run_install_script() -> Result<()> {
    eprintln!();
    eprintln!("{} Installing via install script...", "→".cyan());
    eprintln!();

    let status = Command::new("bash")
        .args(["-c", &format!("curl -fsSL {} | bash", INSTALL_SCRIPT_URL)])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to run install script")?;

    if !status.success() {
        bail!("Install script failed");
    }

    Ok(())
}

/// Check if cargo bin jig exists and prompt for removal
fn check_and_remove_old_cargo_bin() -> Result<()> {
    let home = std::env::var("HOME").unwrap_or_default();
    let cargo_bin = PathBuf::from(&home).join(".cargo/bin/jig");

    if cargo_bin.exists() {
        eprintln!();
        eprintln!(
            "Found old build at {}",
            cargo_bin.display().to_string().dimmed()
        );

        if prompt_confirm("Remove it?", true)? {
            std::fs::remove_file(&cargo_bin).context("Failed to remove old binary")?;
            eprintln!("{} Removed {}", "✓".green(), cargo_bin.display());
        }
    }

    Ok(())
}

/// Compare semver versions (simple implementation)
fn is_newer_version(current: &str, latest: &str) -> bool {
    let parse_version = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = v
            .trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect();
        (
            *parts.first().unwrap_or(&0),
            *parts.get(1).unwrap_or(&0),
            *parts.get(2).unwrap_or(&0),
        )
    };

    let current = parse_version(current);
    let latest = parse_version(latest);

    latest > current
}

pub fn run(force: bool) -> Result<()> {
    let install_method = detect_installation()?;
    let current_version = env!("CARGO_PKG_VERSION");

    // Print header
    eprintln!("{}", "Update".bold());
    eprintln!("  Current version: {}", current_version.to_string().cyan());
    eprintln!("  Installation: {}", install_method.description().dimmed());
    eprintln!();

    // Check for latest version
    eprintln!("{} Checking for updates...", "→".cyan());
    let latest_version = get_latest_version()?;
    eprintln!("  Latest version: {}", latest_version.cyan());
    eprintln!();

    // Compare versions
    let needs_update = is_newer_version(current_version, &latest_version);

    if !needs_update && !force {
        eprintln!("{} Already up to date!", "✓".green());
        return Ok(());
    }

    if needs_update {
        eprintln!(
            "{} New version available: {} → {}",
            "→".cyan(),
            current_version.dimmed(),
            latest_version.green()
        );
    } else {
        eprintln!("{} Forcing update...", "→".cyan());
    }

    match install_method {
        InstallMethod::Script(_) => {
            // Auto-update for script installations
            run_install_script()?;
        }
        InstallMethod::Cargo(_) | InstallMethod::Source(_) => {
            // Prompt for dev builds
            eprintln!();
            eprintln!("You're running a development build.");

            if prompt_confirm("Install latest release to ~/.local/bin?", true)? {
                run_install_script()?;

                // Check for old cargo bin if this was a cargo install
                if matches!(install_method, InstallMethod::Cargo(_)) {
                    // Don't remove the current binary - that would be confusing
                    // The user might still want to keep their dev setup
                } else {
                    // For source builds, check if there's an old cargo bin to clean up
                    check_and_remove_old_cargo_bin()?;
                }
            } else {
                eprintln!();
                eprintln!("To update manually, run:");
                eprintln!(
                    "  {} cargo install --git https://github.com/{}",
                    "→".dimmed(),
                    GITHUB_REPO
                );
                return Ok(());
            }
        }
        InstallMethod::Unknown(ref path) => {
            // Show manual instructions for unknown installations
            eprintln!();
            eprintln!(
                "Unknown installation method: {}",
                path.display().to_string().dimmed()
            );
            eprintln!();
            eprintln!("To install via script (recommended):");
            eprintln!(
                "  {} curl -fsSL {} | bash",
                "→".dimmed(),
                INSTALL_SCRIPT_URL
            );
            eprintln!();
            eprintln!("Or rebuild from source:");
            eprintln!(
                "  {} cargo install --git https://github.com/{}",
                "→".dimmed(),
                GITHUB_REPO
            );
            return Ok(());
        }
    }

    eprintln!();
    eprintln!("{} Updated successfully!", "✓".green());

    Ok(())
}
