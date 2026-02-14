# Auto-spawn blocked by interactive permission prompt

**Status:** Planned

## Objective

When running `jig spawn <name> -c <context> --auto`, Claude Code still presents an interactive confirmation prompt for dangerous tool permissions, blocking the autonomous workflow.

## Background

The `--auto` flag causes jig to pass `--dangerously-skip-permissions` to the `claude` CLI. However, Claude Code still requires the user to interactively acknowledge this mode before proceeding. Since the command runs in a tmux window, the user must manually switch to that window and confirm the prompt — defeating the purpose of autonomous spawning.

## Expected Behavior

`jig spawn <name> -c <context> --auto` should launch Claude Code fully autonomously with no interactive prompts blocking execution.

## Possible Approaches

1. **Pipe confirmation to stdin** — Send a `y` or `Enter` keystroke via `tmux send-keys` after launching Claude to dismiss the prompt automatically.
2. **Use a Claude Code flag or env var** — Check if Claude Code supports a way to skip the confirmation entirely (e.g., an environment variable or additional CLI flag).
3. **Use `--yes` or equivalent** — Claude Code may support a `--yes` flag to auto-accept the dangerous permissions prompt.

## Files

- `crates/jig-core/src/adapter.rs` — Builds the spawn command, sets `auto_flag: "--dangerously-skip-permissions"`
- `crates/jig-core/src/spawn.rs` — Launches tmux window and sends command
- `crates/jig-core/src/session.rs` — `send_keys` function for tmux interaction

## Acceptance Criteria

- [ ] `jig spawn <name> -c <context> --auto` runs Claude Code without any interactive blocking prompts
- [ ] No manual intervention required in the spawned tmux window
- [ ] Behavior is documented or discoverable via `--help`

## Verification

```bash
jig spawn test-worker -c "echo hello" --auto
# Worker should start and begin executing autonomously without manual intervention
```
