# Hook Registry Storage

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/git-hooks/index.md  
**Depends-On:** issues/epics/git-hooks/0-hook-wrapper-pattern.md

## Objective

Implement hook registry that tracks installed hooks, enabling idempotent init and safe uninstall.

## Background

Registry tracks:
- Which hooks are installed by jig
- When they were installed
- Whether user hooks existed and were backed up
- Backup locations for rollback

## Design

### Registry File

Location: `.git/jig-hooks.json`

Schema:
```json
{
  "version": "1",
  "installed": {
    "post-commit": {
      "installed_at": "2026-02-19T10:00:00Z",
      "jig_version": "0.6.0",
      "had_existing": true,
      "backed_up_to": ".git/hooks/post-commit.backup-2026-02-19"
    },
    "post-merge": {
      "installed_at": "2026-02-19T10:00:00Z",
      "jig_version": "0.6.0",
      "had_existing": false
    }
  }
}
```

### Data Structures

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistry {
    pub version: String,
    pub installed: HashMap<String, HookEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    pub installed_at: String,  // ISO 8601 timestamp
    pub jig_version: String,
    pub had_existing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backed_up_to: Option<String>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            version: "1".to_string(),
            installed: HashMap::new(),
        }
    }
    
    pub fn load(repo_path: &Path) -> Result<Self> {
        let path = repo_path.join(".git/jig-hooks.json");
        
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = std::fs::read_to_string(&path)?;
        let registry = serde_json::from_str(&content)?;
        Ok(registry)
    }
    
    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let path = repo_path.join(".git/jig-hooks.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
    
    pub fn mark_installed(&mut self, hook_name: &str) {
        let entry = HookEntry {
            installed_at: chrono::Utc::now().to_rfc3339(),
            jig_version: env!("CARGO_PKG_VERSION").to_string(),
            had_existing: false,
            backed_up_to: None,
        };
        self.installed.insert(hook_name.to_string(), entry);
    }
    
    pub fn mark_existing_backed_up(&mut self, hook_name: &str, backup_name: &str) {
        if let Some(entry) = self.installed.get_mut(hook_name) {
            entry.had_existing = true;
            entry.backed_up_to = Some(backup_name.to_string());
        }
    }
    
    pub fn is_installed(&self, hook_name: &str) -> bool {
        self.installed.contains_key(hook_name)
    }
    
    pub fn remove(&mut self, hook_name: &str) -> Option<HookEntry> {
        self.installed.remove(hook_name)
    }
}
```

## Implementation

**Files:**
- `crates/jig-core/src/hooks/registry.rs` - registry implementation
- `crates/jig-core/src/hooks/mod.rs` - export registry

**Add dependencies to `Cargo.toml`:**
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
```

## Acceptance Criteria

- [ ] `HookRegistry` struct with serde support
- [ ] `load()` creates new registry if file doesn't exist
- [ ] `save()` writes JSON to `.git/jig-hooks.json`
- [ ] `mark_installed()` records hook installation
- [ ] `mark_existing_backed_up()` records backup info
- [ ] `is_installed()` checks if hook is managed by jig
- [ ] `remove()` removes hook from registry
- [ ] Pretty-printed JSON output

## Testing

```rust
#[test]
fn test_registry_new() {
    let registry = HookRegistry::new();
    assert_eq!(registry.version, "1");
    assert!(registry.installed.is_empty());
}

#[test]
fn test_mark_installed() {
    let mut registry = HookRegistry::new();
    registry.mark_installed("post-commit");
    assert!(registry.is_installed("post-commit"));
}

#[test]
fn test_save_load_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let mut registry = HookRegistry::new();
    registry.mark_installed("post-commit");
    
    registry.save(temp.path()).unwrap();
    let loaded = HookRegistry::load(temp.path()).unwrap();
    
    assert!(loaded.is_installed("post-commit"));
}
```

## Next Steps

After this ticket:
- Move to ticket 2 (idempotent init)
- Init will use registry to check existing installations
- Init will update registry when installing hooks
