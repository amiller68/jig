# Dependency Blocking

**Status:** Complete
**Priority:** Medium
**Category:** Improvements

## Objective

Use the existing `depends_on` field to block auto-spawning of issues whose dependencies aren't resolved. The field is already parsed by both providers but never checked.

## Current State

- `Issue.depends_on: Vec<String>` is populated by both providers
- File provider parses `**Depends-On:**` frontmatter
- Linear provider maps `blocks` relations
- Neither `list_spawnable()` nor the daemon checks dependencies before spawning

## Design

### Dependency resolution

For file provider, check if the dependency issue has `status: Complete`:

```rust
fn is_dependency_satisfied(&self, dep_id: &str) -> bool {
    match self.get(dep_id) {
        Ok(Some(issue)) => issue.status == IssueStatus::Complete,
        _ => false, // can't resolve = not satisfied
    }
}
```

For Linear provider, check if the blocking issue's state type is `completed`.

### Apply in list_spawnable

```rust
fn list_spawnable(&self) -> Result<Vec<Issue>> {
    let all = self.scan_all()?;
    Ok(all.into_iter()
        .filter(|i| i.auto && i.status == IssueStatus::Planned)
        .filter(|i| i.depends_on.iter().all(|d| self.is_dependency_satisfied(d)))
        .collect())
}
```

### CLI: show blocked issues

Add `--blocked` / `--unblocked` flags to `jig issues`:

```rust
#[arg(long)]
pub blocked: bool,
#[arg(long)]
pub unblocked: bool,
```

## Acceptance Criteria

- [ ] `list_spawnable()` skips issues with unresolved dependencies
- [ ] File provider resolves local dependencies by checking status
- [ ] Linear provider resolves dependencies via relation state
- [ ] `jig issues --blocked` shows only blocked issues
- [ ] `jig issues --unblocked` shows only unblocked issues
- [ ] Cross-repo dependencies deferred (out of scope for this ticket)
