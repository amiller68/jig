# Linear profile-level filters with per-repo overrides

**Status:** Planned
**Labels:** auto

## Objective

Allow Linear profiles in global config to specify default filters (team, projects, assignee) so repos using that profile inherit sensible defaults without repeating config.

## Context

Currently `jig.toml` requires `team` and optionally `projects` under `[issues.linear]`. There's no way to filter by assignee at all. Teams with multiple repos on the same Linear workspace repeat the same team/project config everywhere.

Filters should live on the profile (global config) as defaults, with per-repo `jig.toml` able to override or narrow them.

## Design

### Global config (`~/.config/jig/config.toml`)

```toml
[linear.profiles.work]
api_key = "lin_api_xxxxxxxxxxxx"
team = "ENG"                          # default team
projects = ["Backend", "Platform"]    # default project filter
assignee = "me"                       # "me" resolves to API key owner, or use email/display name
```

### Per-repo config (`jig.toml`)

```toml
[issues]
provider = "linear"

[issues.linear]
profile = "work"
# team = "ENG"          # override profile default
# projects = ["Backend"] # narrow profile default
# assignee = "alice@co.com" # override profile default
```

### Resolution order

Per-repo `jig.toml` field > profile default > omitted (no filter).

### `assignee = "me"`

Special value that resolves to the authenticated user (via Linear's `viewer` query). Useful for personal issue feeds — only see issues assigned to you.

## Implementation

1. Add optional `team`, `projects`, `assignee` fields to `LinearProfile` in `crates/jig-core/src/global/config.rs`
2. Add optional `assignee` field to `LinearIssuesConfig` in `crates/jig-core/src/config.rs`
3. Make `team` optional in `LinearIssuesConfig` (can fall back to profile)
4. Add resolution logic: merge profile defaults with repo overrides in `LinearProvider::from_config`
5. Add `assignee` filter to the GraphQL query in `linear_client.rs` — use `assignee: { email: { eq: "..." } }` or resolve `"me"` via `viewer { id }` query
6. Update wiki `appendix/linear-integration.md` with new config options

## Files

- `crates/jig-core/src/global/config.rs` — Add filter fields to `LinearProfile`
- `crates/jig-core/src/config.rs` — Add `assignee`, make `team` optional in `LinearIssuesConfig`
- `crates/jig-core/src/issues/linear_provider.rs` — Merge profile + repo config
- `crates/jig-core/src/issues/linear_client.rs` — Add assignee filter to GraphQL query, add `viewer` query for "me" resolution
- `wiki/appendix/linear-integration.md` — Document new config

## Acceptance Criteria

- [ ] Profile-level `team`, `projects`, `assignee` are used as defaults
- [ ] Per-repo `jig.toml` can override any profile-level filter
- [ ] `assignee = "me"` resolves to the API key owner
- [ ] `jig issues` correctly filters by assignee
- [ ] Repos with no `team` in `jig.toml` fall back to profile `team`
- [ ] Existing configs without profile-level filters continue to work unchanged

## Verification

```bash
# Profile has team=ENG, assignee=me
# Repo jig.toml just has profile = "work"
jig issues              # shows only my ENG issues
jig issues --status planned  # filtered further by status

# Repo overrides assignee
# [issues.linear]
# profile = "work"
# assignee = "alice@co.com"
jig issues              # shows Alice's ENG issues
```
