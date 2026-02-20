# Hook Wrapper Pattern

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/git-hooks/index.md

## Objective

Design and implement the hook wrapper pattern that allows jig to coexist with user hooks without conflicts.

## Background

Users might already have git hooks installed. We need to:
- Preserve existing hooks
- Chain jig logic before user hooks
- Make it clear what's jig-managed vs user-managed

## Design

### Wrapper Structure

Generated hook at `.git/hooks/post-commit`:
```bash
#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Run jig handler
if command -v jig &> /dev/null; then
    jig hooks post-commit "$@" || true
fi

# Run user hook if it exists
if [ -f .git/hooks/post-commit.user ]; then
    .git/hooks/post-commit.user "$@"
fi
```

### User Hook Location

User's original/custom hooks go in `.user` suffix:
- `.git/hooks/post-commit.user`
- `.git/hooks/pre-commit.user`
- `.git/hooks/post-merge.user`

### Marker System

First line: `# jig-managed: v1`
- Identifies jig-installed hooks
- Version number for future upgrade logic
- Used by `jig init` to detect existing installation

## Implementation

**Files:**
- `crates/jig-core/src/hooks/templates.rs` - hook templates
- `crates/jig-core/src/hooks/mod.rs` - wrapper generation

**Hook templates as constants:**
```rust
pub const POST_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

if command -v jig &> /dev/null; then
    jig hooks post-commit "$@" || true
fi

if [ -f .git/hooks/post-commit.user ]; then
    .git/hooks/post-commit.user "$@"
fi
"#;

pub const POST_MERGE_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
set -e
if command -v jig &> /dev/null; then
    jig hooks post-merge "$@" || true
fi
if [ -f .git/hooks/post-merge.user ]; then
    .git/hooks/post-merge.user "$@"
fi
"#;

pub const PRE_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
set -e
# Pre-commit can fail and block commit
if command -v jig &> /dev/null; then
    jig hooks pre-commit "$@"
fi
if [ -f .git/hooks/pre-commit.user ]; then
    .git/hooks/pre-commit.user "$@"
fi
"#;
```

**Wrapper generation function:**
```rust
pub fn generate_hook(hook_name: &str) -> Result<String> {
    match hook_name {
        "post-commit" => Ok(POST_COMMIT_TEMPLATE.to_string()),
        "post-merge" => Ok(POST_MERGE_TEMPLATE.to_string()),
        "pre-commit" => Ok(PRE_COMMIT_TEMPLATE.to_string()),
        _ => Err(Error::UnsupportedHook(hook_name.to_string())),
    }
}

pub fn is_jig_managed(content: &str) -> bool {
    content.lines().next()
        .map(|line| line.contains("# jig-managed:"))
        .unwrap_or(false)
}
```

## Acceptance Criteria

- [ ] Hook templates defined as constants
- [ ] `generate_hook()` returns template for hook name
- [ ] `is_jig_managed()` detects jig-managed hooks
- [ ] Templates include marker comment
- [ ] Templates call `jig hooks <name>` handler
- [ ] Templates chain to `.user` suffix if exists
- [ ] Pre-commit can fail (exits with error code)
- [ ] Post-commit/merge never fail (|| true)

## Testing

```rust
#[test]
fn test_generate_post_commit() {
    let hook = generate_hook("post-commit").unwrap();
    assert!(hook.contains("# jig-managed: v1"));
    assert!(hook.contains("jig hooks post-commit"));
    assert!(hook.contains(".git/hooks/post-commit.user"));
}

#[test]
fn test_is_jig_managed() {
    let content = "# jig-managed: v1\nrest of hook";
    assert!(is_jig_managed(content));
    
    let user_content = "#!/bin/bash\necho 'user hook'";
    assert!(!is_jig_managed(user_content));
}
```

## Next Steps

After this ticket:
- Move to ticket 1 (registry storage)
- Registry will track which hooks are installed
- Init will use these templates to install hooks

## Progress Log

### 2026-02-19 - Started
- Beginning implementation
