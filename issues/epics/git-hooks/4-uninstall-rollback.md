# Uninstall & Rollback

**Status:** Planned  
**Priority:** Medium  
**Category:** Features  
**Epic:** issues/epics/git-hooks/index.md  
**Depends-On:** issues/epics/git-hooks/3-hook-handlers.md

## Objective

Implement `jig hooks uninstall` that safely removes jig hooks and restores user's original hooks.

## Background

Users should be able to cleanly remove jig hooks:
- Restore original hooks from backups
- Move `.user` hooks back to main hook path
- Delete jig-managed hooks
- Clean up registry
- Leave repo in clean state

## Design

### Uninstall Flow

```
1. Load registry from .git/jig-hooks.json
2. For each installed hook in registry:
   a. Remove jig hook wrapper
   b. If backup exists: restore it
   c. Else if .user hook exists: move to main hook path
   d. Remove from registry
3. Delete .git/jig-hooks.json
4. Report what was restored
```

### Rollback Logic

```rust
pub fn uninstall_hook(
    hooks_dir: &Path,
    hook_name: &str,
    entry: &HookEntry
) -> Result<UninstallResult> {
    let hook_path = hooks_dir.join(hook_name);
    
    // Remove jig hook
    if hook_path.exists() {
        std::fs::remove_file(&hook_path)?;
    }
    
    // Restore from backup if exists
    if let Some(backup_name) = &entry.backed_up_to {
        let backup_path = hooks_dir.join(backup_name);
        if backup_path.exists() {
            std::fs::copy(&backup_path, &hook_path)?;
            return Ok(UninstallResult::RestoredBackup(backup_name.clone()));
        }
    }
    
    // Or restore from .user suffix
    let user_path = hooks_dir.join(format!("{}.user", hook_name));
    if user_path.exists() {
        std::fs::rename(&user_path, &hook_path)?;
        return Ok(UninstallResult::RestoredUser);
    }
    
    Ok(UninstallResult::Removed)
}

pub enum UninstallResult {
    Removed,                       // Removed, no previous hook
    RestoredBackup(String),        // Restored from backup file
    RestoredUser,                  // Moved .user back to main path
}
```

## Implementation

**Commands:**
- `jig hooks uninstall` - remove all hooks
- `jig hooks uninstall post-commit` - remove specific hook

**Files:**
- `crates/jig-cli/src/commands/hooks.rs` - add uninstall subcommand
- `crates/jig-core/src/hooks/uninstall.rs` - uninstall logic

**Core function:**
```rust
pub fn uninstall_hooks(repo_path: &Path, specific_hook: Option<&str>) -> Result<Vec<UninstallResult>> {
    let hooks_dir = repo_path.join(".git/hooks");
    let registry_path = repo_path.join(".git/jig-hooks.json");
    
    let mut registry = HookRegistry::load(repo_path)?;
    let mut results = Vec::new();
    
    // Determine which hooks to uninstall
    let hooks_to_remove: Vec<String> = if let Some(hook) = specific_hook {
        vec![hook.to_string()]
    } else {
        registry.installed.keys().cloned().collect()
    };
    
    for hook_name in hooks_to_remove {
        if let Some(entry) = registry.remove(&hook_name) {
            let result = uninstall_hook(&hooks_dir, &hook_name, &entry)?;
            results.push((hook_name, result));
        }
    }
    
    // If uninstalling all hooks, remove registry file
    if specific_hook.is_none() && registry.installed.is_empty() {
        if registry_path.exists() {
            std::fs::remove_file(&registry_path)?;
        }
    } else {
        // Save updated registry
        registry.save(repo_path)?;
    }
    
    Ok(results)
}
```

## Acceptance Criteria

- [ ] `jig hooks uninstall` removes all jig hooks
- [ ] `jig hooks uninstall post-commit` removes specific hook
- [ ] Backups restored if they exist
- [ ] `.user` hooks moved back to main path if no backup
- [ ] Registry updated or deleted
- [ ] Friendly output showing what was restored
- [ ] No errors if hooks already removed

## Output Format

```
$ jig hooks uninstall

Uninstalling jig hooks...

✓ post-commit: removed wrapper, restored backup from 2026-02-19
✓ post-merge: removed wrapper (no previous hook)
✓ pre-commit: removed wrapper, restored .user hook

All jig hooks uninstalled.
Your original hooks have been restored.
```

## Testing

```rust
#[test]
fn test_uninstall_clean() {
    let repo = TestRepo::new();
    
    // Install hooks
    init_hooks(&repo.path(), false).unwrap();
    
    // Uninstall
    let results = uninstall_hooks(&repo.path(), None).unwrap();
    assert_eq!(results.len(), 3);
    
    // Hooks should be gone
    assert!(!repo.path().join(".git/hooks/post-commit").exists());
    assert!(!repo.path().join(".git/jig-hooks.json").exists());
}

#[test]
fn test_uninstall_restores_backup() {
    let repo = TestRepo::new();
    
    // Create user hook
    let hook_path = repo.path().join(".git/hooks/post-commit");
    std::fs::write(&hook_path, "#!/bin/bash\necho 'original'").unwrap();
    
    // Install (backs up)
    init_hooks(&repo.path(), false).unwrap();
    
    // Uninstall (restores)
    uninstall_hooks(&repo.path(), None).unwrap();
    
    // Original hook should be back
    let content = std::fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains("echo 'original'"));
}

#[test]
fn test_uninstall_specific_hook() {
    let repo = TestRepo::new();
    init_hooks(&repo.path(), false).unwrap();
    
    // Uninstall just one hook
    uninstall_hooks(&repo.path(), Some("post-commit")).unwrap();
    
    // post-commit gone, others remain
    assert!(!repo.path().join(".git/hooks/post-commit").exists());
    assert!(repo.path().join(".git/hooks/post-merge").exists());
    
    // Registry still exists
    let registry = HookRegistry::load(&repo.path()).unwrap();
    assert!(!registry.is_installed("post-commit"));
    assert!(registry.is_installed("post-merge"));
}
```

## Edge Cases

**Multiple uninstalls:**
```bash
jig hooks uninstall  # First time
jig hooks uninstall  # Second time - should be no-op, no errors
```

**Missing backups:**
If backup file was deleted manually, fall back to `.user` hook or clean removal.

**Partial state:**
If registry exists but hooks don't (manual deletion), clean up registry without errors.

## CLI Integration

```rust
#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommand {
    Install { #[arg(long)] force: bool },
    Uninstall { hook: Option<String> },
    List,
    Status,
}
```

## Next Steps

After this ticket:
- Git hooks epic is COMPLETE!
- Worker heartbeat system can now use git hooks
- Health checks can be triggered on commit/merge
