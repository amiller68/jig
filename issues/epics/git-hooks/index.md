# Epic: Git Hooks Management

**Status:** Planned  
**Priority:** High  
**Category:** Features

## Objective

Implement robust git hooks installation, management, and idempotent `jig init` that safely installs/updates hooks without breaking existing user hooks.

## Why This Matters

Git hooks enable:
- Updating worker metrics on commit/merge (heartbeat system needs this)
- Triggering health checks after operations
- Enforcing conventional commits (optional)
- Auto-updating issue status

## Tickets

| # | Ticket | Status | Priority |
|---|--------|--------|----------|
| 0 | Hook wrapper pattern | Planned | High |
| 1 | Registry storage | Planned | High |
| 2 | Idempotent init | Planned | High |
| 3 | Hook handlers | Planned | High |
| 4 | Uninstall & rollback | Planned | Medium |

## Design Principles

1. **Never break user hooks** - use wrapper pattern with `.user` suffix
2. **Idempotent** - safe to run `jig init` multiple times
3. **Trackable** - registry tracks what's installed
4. **Reversible** - clean uninstall with backup restoration
5. **Adapter-agnostic** - works with any agent (Claude, Cursor, etc.)

## Dependencies

None (foundational epic)

## Acceptance Criteria

- [ ] `jig init` installs hooks safely
- [ ] Running `jig init` twice is safe (idempotent)
- [ ] Existing user hooks are preserved in `.user` suffix
- [ ] Hooks update worker metrics on commit/merge
- [ ] `jig hooks uninstall` restores original state
- [ ] All hooks are adapter-agnostic

## References

- Original spec: `docs/GRINDER-ANALYSIS.md` (git hooks section)
