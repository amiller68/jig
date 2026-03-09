# Labels and Tags

**Status:** Planned
**Priority:** High
**Category:** Improvements
**Auto:** true

## Objective

Add a `labels` field to the `Issue` type so issues can be tagged and filtered by label. This unblocks label-based auto-spawn targeting (e.g., "only spawn issues tagged `backend`").

## Current State

- Linear provider already fetches `labels { nodes { name } }` from GraphQL, but only uses them to detect the `jig-auto` label for the `auto` field. The actual label names are discarded.
- File provider has no label/tag support at all.
- `Issue` struct has no `labels` field.
- `IssueFilter` has no label filter.
- CLI `jig issues` has no `--label` flag.

## Design

### Issue struct

```rust
pub struct Issue {
    // ... existing fields ...
    pub labels: Vec<String>,  // NEW
}
```

### IssueFilter

```rust
pub struct IssueFilter {
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub category: Option<String>,
    pub labels: Vec<String>,  // NEW: all must match
}
```

### Linear provider

Already fetches labels — just needs to populate `issue.labels` from the GraphQL response instead of only checking for `jig-auto`. The `auto` field should remain derived from the presence of `jig-auto` in labels.

In `linear_client.rs`, the `RawIssue` already has `labels: Vec<String>`. Just pass them through:

```rust
Issue {
    // ...
    auto: labels.iter().any(|l| l.eq_ignore_ascii_case("jig-auto")),
    labels,  // NEW: pass all labels through
}
```

For `list()` filtering: if `filter.labels` is non-empty, filter results client-side (Linear's GraphQL label filter is awkward for "all must match").

### File provider

Parse `**Labels:**` frontmatter field (comma-separated):

```markdown
**Labels:** backend, auth, sprint-12
```

```rust
let labels = extract_field(&content, "Labels")
    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect())
    .unwrap_or_default();
```

### CLI

Add `--label/-l` flag to `jig issues`:

```rust
/// Filter by label (can specify multiple)
#[arg(short, long)]
pub label: Vec<String>,
```

### Shell completions

Add `--label` / `-l` to issues completions in all three shells.

## Acceptance Criteria

- [ ] `Issue.labels: Vec<String>` field added
- [ ] `IssueFilter.labels: Vec<String>` field added
- [ ] Linear provider populates `labels` from GraphQL response
- [ ] File provider parses `**Labels:**` comma-separated frontmatter
- [ ] `jig issues --label backend` filters by label
- [ ] Multiple labels: `jig issues --label backend --label auth` requires all to match
- [ ] Shell completions updated for `--label`
- [ ] `auto` field derivation unchanged (still uses `jig-auto` / `**Auto:**`)
