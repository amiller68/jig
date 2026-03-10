# Spawn preamble: provider-aware issue completion instructions

**Status:** Planned
**Auto:** true

## Objective

Update the auto-spawn preamble so workers know where their issue came from and how to mark it done, preventing infinite re-spawn loops.

## Problem

The daemon's `list_spawnable()` returns issues with `status == Planned` and `auto == true`. If a worker completes its task but never updates the issue status, the daemon will re-spawn a new worker for the same issue after the original worker is pruned. The current preamble says nothing about issue completion.

## Implementation

1. Add `provider_name: String` to `SpawnableIssue` in `crates/jig-core/src/daemon/messages.rs`
2. Populate `provider_name` from `provider.name()` in `crates/jig-core/src/daemon/issue_actor.rs` when building `SpawnableIssue` entries
3. In `auto_spawn_worker()` (`crates/jig-core/src/daemon/mod.rs`), append provider-specific completion instructions to the context string:
   - **File provider:** Instruct the worker to change `**Status:** Planned` to `**Status:** Complete` in the issue markdown file and commit the change
   - **Linear provider:** Note that status sync is handled separately (or instruct worker not to worry about manual status updates)
4. Update `SPAWN_PREAMBLE` in `crates/jig-core/src/templates/builtin.rs` to mention that marking the issue as done is part of the definition of done

## Files

- `crates/jig-core/src/daemon/messages.rs` — Add `provider_name` field to `SpawnableIssue`
- `crates/jig-core/src/daemon/issue_actor.rs` — Set `provider_name` from `provider.name()`
- `crates/jig-core/src/daemon/mod.rs` — Append provider-specific completion text to context in `auto_spawn_worker()`
- `crates/jig-core/src/templates/builtin.rs` — Update `SPAWN_PREAMBLE` definition of done

## Acceptance Criteria

- [ ] `SpawnableIssue` carries the provider name
- [ ] File-provider workers are told to update `**Status:** Complete` in the issue file
- [ ] Linear-provider workers get appropriate completion instructions
- [ ] Preamble definition of done includes marking the issue complete
- [ ] `cargo build && cargo test && cargo clippy` pass

## Verification

1. `cargo build` compiles
2. `cargo test` — existing tests pass, `derive_worker_name` tests still work
3. Inspect rendered preamble text for a file-provider issue and verify completion instructions are present
