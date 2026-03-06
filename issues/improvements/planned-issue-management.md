# Planned Issue Management

**Status:** In Progress
**Priority:** Medium
**Category:** Improvements

## Background

Issue management is partially implemented. Core discovery, parsing, filtering (status/priority/category), and basic auto-spawn all work for both file and Linear providers. This epic tracks the remaining gaps.

## What's Done

- File provider: recursive scan, frontmatter parsing (status, priority, category, depends-on, auto)
- Linear provider: GraphQL queries, priority/status/project mapping, `jig-auto` label detection
- `jig issues` CLI: filter by status, priority, category; interactive expand; `--ids` for scripting
- Daemon auto-spawn: polls on interval, spawns `auto=true` + `status=Planned` issues, respects max workers
- Issue types: Issue struct with id, title, status, priority, category, depends_on, body, children, auto

## What's Missing

See child tickets:

| # | Ticket | Priority | Status |
|---|--------|----------|--------|
| 1 | [Auto-spawn filtering](./auto-spawn-filtering.md) | High | Planned |
| 2 | [Labels and tags](./labels-and-tags.md) | High | Planned |
| 3 | [Dependency blocking](./dependency-blocking.md) | Medium | Planned |
| 4 | [Batch spawning](./batch-spawning.md) | Low | Planned |
| 5 | [Issue lifecycle commands](./issue-lifecycle-commands.md) | Low | Planned |
