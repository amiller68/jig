//! Worktree lifecycle hooks and file operations.

use std::path::Path;

use super::repo::JigToml;
use jig_core::error::Result;

/// Run on-create hook in a directory
pub fn run_on_create_hook(hook: &str, dir: &Path) -> Result<bool> {
    tracing::info!("Running on-create hook: {}", hook);

    let output = std::process::Command::new("sh")
        .args(["-c", hook])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        tracing::warn!(
            "on-create hook failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(false);
    }

    Ok(true)
}

/// Run on-create hook if configured for a repo (from jig.toml).
pub fn run_on_create_hook_for_repo(repo_root: &Path, worktree_path: &Path) -> Result<()> {
    let hook = if let Some(jig_toml) = JigToml::load(repo_root)? {
        jig_toml.worktree.on_create
    } else {
        None
    };

    if let Some(hook) = hook {
        let success = run_on_create_hook(&hook, worktree_path)?;
        if !success {
            tracing::warn!("on-create hook returned non-zero exit code");
        }
    }

    Ok(())
}

/// Get list of files to copy to new worktrees
pub fn get_copy_files(repo_root: &Path) -> Result<Vec<String>> {
    if let Some(jig_toml) = JigToml::load(repo_root)? {
        Ok(jig_toml.worktree.copy)
    } else {
        Ok(Vec::new())
    }
}

/// Copy configured files from source to destination
pub fn copy_worktree_files(src_root: &Path, dst_root: &Path, files: &[String]) -> Result<()> {
    for file in files {
        let src = src_root.join(file);
        let dst = dst_root.join(file);

        if src.exists() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
            tracing::info!("Copied {} to worktree", file);
        }
    }
    Ok(())
}
