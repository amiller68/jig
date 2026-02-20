# Idempotent Init

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/git-hooks/index.md  
**Depends-On:** issues/epics/git-hooks/1-registry-storage.md

## Objective

Implement `jig init` command that safely installs hooks with idempotent behavior (safe to run multiple times).

## Background

`jig init` must:
- Install jig hooks on first run
- Do nothing (or update safely) on subsequent runs
- Back up existing user hooks
- Move user hooks to `.user` suffix
- Never break existing hooks

## Design

### Init Flow

```
1. Load registry from .git/jig-hooks.json (or create new)
2. For each hook (post-commit, post-merge, pre-commit):
   a. Check if .git/hooks/<hook> exists
   b. If exists:
      - Read content
      - If jig-managed: check if up-to-date, skip or update
      - If user hook: back up, move to .user suffix
   c. Write jig wrapper hook
   d. chmod +x
   e. Update registry
3. Save registry
```

### Idempotency Logic

```rust
pub fn should_install_hook(
    hook_path: &Path,
    registry: &HookRegistry,
    hook_name: &str,
    force: bool
) -> Result<InstallDecision> {
    // If force flag, always reinstall
    if force {
        return Ok(InstallDecision::Reinstall);
    }
    
    // If doesn't exist, install
    if !hook_path.exists() {
        return Ok(InstallDecision::Install);
    }
    
    // Read existing hook
    let content = std::fs::read_to_string(hook_path)?;
    
    // If jig-managed, check if up-to-date
    if is_jig_managed(&content) {
        if registry.is_installed(hook_name) {
            // Already installed, skip
            return Ok(InstallDecision::Skip);
        } else {
            // Jig-managed but not in registry, update
            return Ok(InstallDecision::UpdateRegistry);
        }
    }
    
    // User hook exists, need to back up
    Ok(InstallDecision::BackupAndInstall)
}

pub enum InstallDecision {
    Install,             // No hook, install fresh
    Skip,                // Already installed, no action
    Reinstall,           // Force flag, reinstall
    UpdateRegistry,      // Hook exists but registry needs update
    BackupAndInstall,    // User hook, back up and install
}
```

## Implementation

**Commands:**
- `jig init` - install hooks (skip if already installed)
- `jig init --force` - reinstall all hooks

**Files:**
- `crates/jig-cli/src/commands/init.rs` - init command
- `crates/jig-core/src/hooks/install.rs` - installation logic

**Core function:**
```rust
pub fn init_hooks(repo_path: &Path, force: bool) -> Result<InitResult> {
    let hooks_dir = repo_path.join(".git/hooks");
    let mut registry = HookRegistry::load(repo_path)?;
    
    let hooks_to_install = vec!["post-commit", "post-merge", "pre-commit"];
    let mut results = Vec::new();
    
    for hook_name in hooks_to_install {
        let hook_path = hooks_dir.join(hook_name);
        
        let decision = should_install_hook(&hook_path, &registry, hook_name, force)?;
        
        match decision {
            InstallDecision::Skip => {
                results.push(HookResult::AlreadyInstalled(hook_name.to_string()));
                continue;
            },
            InstallDecision::BackupAndInstall => {
                // Back up user hook
                let backup_name = format!("{}.backup-{}", hook_name, now_iso());
                let backup_path = hooks_dir.join(&backup_name);
                std::fs::copy(&hook_path, &backup_path)?;
                
                // Move to .user suffix
                let user_path = hooks_dir.join(format!("{}.user", hook_name));
                std::fs::rename(&hook_path, &user_path)?;
                
                registry.mark_existing_backed_up(hook_name, &backup_name);
            },
            _ => {}
        }
        
        // Install jig hook
        let hook_content = generate_hook(hook_name)?;
        std::fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        
        registry.mark_installed(hook_name);
        results.push(HookResult::Installed(hook_name.to_string()));
    }
    
    registry.save(repo_path)?;
    
    Ok(InitResult { results })
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}
```

## Acceptance Criteria

- [ ] `jig init` installs hooks on first run
- [ ] `jig init` skips already-installed hooks on second run
- [ ] `jig init --force` reinstalls all hooks
- [ ] Existing user hooks backed up to `.backup-*` files
- [ ] User hooks moved to `.user` suffix
- [ ] Hooks are executable (chmod +x)
- [ ] Registry updated with installation details
- [ ] Friendly output showing what was done

## Output Format

```
$ jig init

Installing git hooks...

✓ post-commit: installed
✓ post-merge: installed (backed up existing hook)
✓ pre-commit: installed

Hooks installed successfully.
Your existing hooks have been moved to .git/hooks/*.user

$ jig init

Installing git hooks...

✓ post-commit: already installed
✓ post-merge: already installed
✓ pre-commit: already installed

All hooks are up to date.
```

## Testing

```rust
#[test]
fn test_init_fresh_repo() {
    let repo = TestRepo::new();
    let result = init_hooks(repo.path(), false).unwrap();
    assert_eq!(result.results.len(), 3);
    
    // All hooks should be installed
    assert!(repo.path().join(".git/hooks/post-commit").exists());
}

#[test]
fn test_init_idempotent() {
    let repo = TestRepo::new();
    
    // First init
    init_hooks(repo.path(), false).unwrap();
    
    // Second init should skip
    let result = init_hooks(repo.path(), false).unwrap();
    assert!(result.results.iter().all(|r| matches!(r, HookResult::AlreadyInstalled(_))));
}

#[test]
fn test_init_with_existing_hook() {
    let repo = TestRepo::new();
    
    // Create user hook
    let hook_path = repo.path().join(".git/hooks/post-commit");
    std::fs::write(&hook_path, "#!/bin/bash\necho 'user hook'").unwrap();
    
    // Init should back up and install
    init_hooks(repo.path(), false).unwrap();
    
    // User hook should be in .user suffix
    let user_path = repo.path().join(".git/hooks/post-commit.user");
    assert!(user_path.exists());
    
    // Backup should exist
    let backups: Vec<_> = std::fs::read_dir(repo.path().join(".git/hooks"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("post-commit.backup-"))
        .collect();
    assert_eq!(backups.len(), 1);
}
```

## Next Steps

After this ticket:
- Move to ticket 3 (hook handlers)
- Handlers will implement the actual git hook logic
- Handlers will update worker metrics
