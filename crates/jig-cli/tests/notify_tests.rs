#![allow(deprecated)]
//! Integration tests for `jig notify` subcommands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct TestEnv {
    dir: TempDir,
    config_dir: TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let config_dir = TempDir::new().expect("create config dir");

        // Create a minimal git repo so jig doesn't error on repo detection
        StdCommand::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("git init");

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("set email");

        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("set name");

        let config_file = config_dir.path().join("jig").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").unwrap();

        Self { dir, config_dir }
    }

    fn with_notify_config(self, toml_content: &str) -> Self {
        let config_file = self.config_dir.path().join("jig").join("config.toml");
        fs::write(&config_file, toml_content).unwrap();
        self
    }

    fn queue_path(&self) -> std::path::PathBuf {
        self.config_dir
            .path()
            .join("jig")
            .join("state")
            .join("notifications.jsonl")
    }

    fn seed_queue(&self, lines: &[&str]) {
        let queue_path = self.queue_path();
        fs::create_dir_all(queue_path.parent().unwrap()).unwrap();
        fs::write(&queue_path, lines.join("\n") + "\n").unwrap();
    }

    fn jig(&self) -> Command {
        let mut cmd = Command::cargo_bin("jig").unwrap();
        cmd.current_dir(self.dir.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }
}

// --- doctor ---

#[test]
fn doctor_shows_exec_unset_without_notify_section() {
    let env = TestEnv::new();

    env.jig()
        .args(["notify", "doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("exec: <unset>"));
}

#[test]
fn doctor_shows_configured_exec() {
    let env = TestEnv::new().with_notify_config("[notify]\nexec = \"my-hook.sh\"\n");

    env.jig()
        .args(["notify", "doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("exec: my-hook.sh"));
}

#[test]
fn doctor_shows_events_filter() {
    let env = TestEnv::new()
        .with_notify_config("[notify]\nevents = [\"needs_intervention\", \"pr_opened\"]\n");

    env.jig()
        .args(["notify", "doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("needs_intervention, pr_opened"));
}

#[test]
fn doctor_surfaces_toml_parse_error() {
    let env = TestEnv::new().with_notify_config("[notify\nexec = broken\n");

    env.jig()
        .args(["notify", "doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("TOML parse error"));
}

#[test]
fn doctor_shows_no_queue_when_missing() {
    let env = TestEnv::new();

    env.jig()
        .args(["notify", "doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no queue file"));
}

// --- test ---

#[test]
fn test_emits_notification_to_queue() {
    let env = TestEnv::new();

    env.jig()
        .args(["notify", "test"])
        .assert()
        .success()
        .stderr(predicate::str::contains("emitted test notification"));

    let content = fs::read_to_string(env.queue_path()).unwrap();
    assert!(content.contains("needs_intervention"));
    assert!(content.contains("notify-test"));
}

#[test]
fn test_with_exec_hook_writes_to_file() {
    let env = TestEnv::new();
    let output_path = env.dir.path().join("hook-output.json");

    let toml = format!("[notify]\nexec = \"cat > '{}'\"\n", output_path.display());
    let env = env.with_notify_config(&toml);

    env.jig()
        .args(["notify", "test"])
        .assert()
        .success()
        .stderr(predicate::str::contains("emitted test notification"));

    let hook_output = fs::read_to_string(&output_path).unwrap();
    assert!(hook_output.contains("needs_intervention"));
    assert!(hook_output.contains("notify-test"));
}

#[test]
fn test_with_failing_hook_shows_error() {
    let env = TestEnv::new().with_notify_config("[notify]\nexec = \"exit 1\"\n");

    env.jig()
        .args(["notify", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("notification hook exited with"));
}

// --- tail ---

#[test]
fn tail_shows_seeded_events() {
    let env = TestEnv::new();
    env.seed_queue(&[
        r#"{"ts":1000,"id":"aaa","type":"work_started","repo":"r","worker":"w1","issue":null}"#,
        r#"{"ts":2000,"id":"bbb","type":"needs_intervention","repo":"r","worker":"w2","reason":"stuck"}"#,
        r#"{"ts":3000,"id":"ccc","type":"work_completed","repo":"r","worker":"w3","pr_url":null}"#,
    ]);

    env.jig()
        .args(["notify", "tail", "-n", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("needs_intervention"))
        .stdout(predicate::str::contains("work_completed"))
        .stdout(predicate::str::contains("work_started").not());
}

#[test]
fn tail_empty_queue_shows_no_events() {
    let env = TestEnv::new();

    env.jig()
        .args(["notify", "tail"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no events"));
}

// --- send ---

#[test]
fn send_needs_intervention_writes_to_queue() {
    let env = TestEnv::new();

    env.jig()
        .args([
            "notify",
            "send",
            "needs-intervention",
            "--repo",
            "myrepo",
            "--worker",
            "feat-x",
            "--reason",
            "stuck on build",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("emitted needs_intervention"));

    let content = fs::read_to_string(env.queue_path()).unwrap();
    assert!(content.contains("needs_intervention"));
    assert!(content.contains("myrepo"));
    assert!(content.contains("feat-x"));
    assert!(content.contains("stuck on build"));
}

#[test]
fn send_work_started_writes_to_queue() {
    let env = TestEnv::new();

    env.jig()
        .args([
            "notify",
            "send",
            "work-started",
            "--repo",
            "myrepo",
            "--worker",
            "feat-x",
            "--issue",
            "JIG-99",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("emitted work_started"));

    let content = fs::read_to_string(env.queue_path()).unwrap();
    assert!(content.contains("work_started"));
    assert!(content.contains("JIG-99"));
}

#[test]
fn send_pr_opened_writes_to_queue() {
    let env = TestEnv::new();

    env.jig()
        .args([
            "notify",
            "send",
            "pr-opened",
            "--repo",
            "myrepo",
            "--worker",
            "feat-x",
            "--pr-url",
            "https://github.com/test/pr/1",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("emitted pr_opened"));

    let content = fs::read_to_string(env.queue_path()).unwrap();
    assert!(content.contains("pr_opened"));
    assert!(content.contains("https://github.com/test/pr/1"));
}
