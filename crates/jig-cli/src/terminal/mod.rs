pub mod shell;

use std::path::PathBuf;

/// Resolve a command on `$PATH`, returning its full path if found.
pub fn which(cmd: &str) -> Option<PathBuf> {
    which::which(cmd).ok()
}

/// Result of checking a system dependency.
#[derive(Debug)]
pub struct DepCheck {
    pub name: String,
    pub found: bool,
    pub version: Option<String>,
}

/// Check whether a CLI dependency is available, optionally extracting its version.
///
/// `version_args` are passed to the command to get version output (e.g. `&["--version"]`).
/// The first token of stdout is returned as the version string.
pub fn check_dep(name: &str, version_args: &[&str]) -> DepCheck {
    let found = which(name).is_some();
    let version = if found {
        std::process::Command::new(name)
            .args(version_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    DepCheck { name: name.to_string(), found, version }
}
