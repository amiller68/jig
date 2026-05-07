#![allow(deprecated)] // Command::cargo_bin is deprecated but used across tests

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ============================================================================
// Attach Auto-Detection Tests
// ============================================================================

#[test]
fn test_attach_outside_repo_requires_name() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = TempDir::new().expect("Failed to create config dir");

    let mut cmd = Command::cargo_bin("jig").expect("Failed to find jig binary");
    cmd.current_dir(dir.path());
    cmd.env("XDG_CONFIG_HOME", config_dir.path());
    cmd.args(["attach"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("name is required"));
}

#[test]
fn test_attach_outside_repo_nonexistent_worktree() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = TempDir::new().expect("Failed to create config dir");

    let mut cmd = Command::cargo_bin("jig").expect("Failed to find jig binary");
    cmd.current_dir(dir.path());
    cmd.env("XDG_CONFIG_HOME", config_dir.path());
    cmd.args(["attach", "nonexistent-worker"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
