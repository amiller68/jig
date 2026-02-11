//! Shell setup command - automatically configures shell integration

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Marker comment to identify jig's shell integration block
const MARKER_START: &str = "# >>> jig shell integration >>>";
const MARKER_END: &str = "# <<< jig shell integration <<<";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    fn from_path(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next()?;
        match name {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
        }
    }

    fn config_file(&self) -> PathBuf {
        let home = dirs::home_dir().expect("Could not find home directory");
        match self {
            Shell::Bash => home.join(".bashrc"),
            Shell::Zsh => home.join(".zshrc"),
            Shell::Fish => home.join(".config/fish/config.fish"),
        }
    }

    fn eval_line(&self) -> &'static str {
        match self {
            Shell::Bash | Shell::Zsh => r#"eval "$(jig shell-init {shell})""#,
            Shell::Fish => "jig shell-init fish | source",
        }
    }

    fn integration_block(&self) -> String {
        let eval_line = self.eval_line().replace("{shell}", self.name());
        format!("{}\n{}\n{}\n", MARKER_START, eval_line, MARKER_END)
    }
}

fn detect_shell() -> Result<Shell> {
    let shell_path = env::var("SHELL").context("SHELL environment variable not set")?;
    Shell::from_path(&shell_path).with_context(|| {
        format!(
            "Unsupported shell: {}. Supported: bash, zsh, fish",
            shell_path
        )
    })
}

fn find_path_line_end(content: &str) -> Option<usize> {
    // Look for common PATH setup patterns and return position after the last one
    let path_patterns = [
        "export PATH=",
        "PATH=",
        "path+=",
        "set -gx PATH",
        "fish_add_path",
        // Common tools that modify PATH
        "cargo/env",
        "nvm.sh",
        "rbenv init",
        "pyenv init",
        "eval \"$(brew shellenv)\"",
        "eval (brew shellenv)",
    ];

    let mut last_path_end = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        for pattern in &path_patterns {
            if trimmed.contains(pattern) {
                // Find the byte offset of the end of this line
                let line_end = content
                    .lines()
                    .take(i + 1)
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum::<usize>();
                last_path_end = Some(line_end);
                break;
            }
        }
    }

    last_path_end
}

fn has_existing_integration(content: &str) -> bool {
    content.contains(MARKER_START)
        || content.contains("jig shell-init")
        || content.contains("eval \"$(jig")
}

pub fn run(dry_run: bool) -> Result<()> {
    let shell = detect_shell()?;
    let config_path = shell.config_file();

    println!("Detected shell: {}", shell.name());
    println!("Config file: {}", config_path.display());

    // Read existing config or start fresh
    let existing_content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?
    } else {
        String::new()
    };

    // Check for existing integration
    if has_existing_integration(&existing_content) {
        if existing_content.contains(MARKER_START) {
            println!("\njig shell integration is already configured.");
            println!("To reconfigure, remove the block between:");
            println!("  {}", MARKER_START);
            println!("  {}", MARKER_END);
        } else {
            println!("\njig shell integration appears to be configured (found 'jig shell-init').");
            println!("If it's not working, check that the eval line comes AFTER your PATH setup.");
        }
        return Ok(());
    }

    // Find the best insertion point (after PATH setup)
    let integration_block = shell.integration_block();

    if dry_run {
        println!("\n[Dry run] Would add to {}:", config_path.display());
        println!("{}", integration_block.trim());
        return Ok(());
    }

    let new_content = if let Some(insert_pos) = find_path_line_end(&existing_content) {
        // Insert after the last PATH-related line
        let (before, after) = existing_content.split_at(insert_pos);
        format!("{}\n{}{}", before.trim_end(), integration_block, after)
    } else {
        // No PATH setup found, append to end
        if existing_content.is_empty() {
            integration_block
        } else {
            format!("{}\n\n{}", existing_content.trim_end(), integration_block)
        }
    };

    // Ensure parent directory exists (for fish)
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    // Write the updated config
    fs::write(&config_path, &new_content)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    println!("\nAdded jig shell integration to {}", config_path.display());
    println!("\nTo activate, either:");
    println!("  1. Open a new terminal, or");
    println!("  2. Run: source {}", config_path.display());
    println!("\nVerify with: type jig");
    println!("Expected output: \"jig is a shell function\" (or similar)");

    Ok(())
}
