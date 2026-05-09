# Daemon-to-Service Refactor Plan

Turn the in-process daemon into a persistent background service with IPC, managed via `service-manager`.

## Design Decisions

- **IPC**: JSON-over-Unix-domain-socket, newline-delimited (matches existing JSONL event log pattern, stdlib only)
- **Socket path**: `$XDG_RUNTIME_DIR/jig/daemon.sock`, falling back to `~/.config/jig/state/daemon.sock`
- **Single instance**: One daemon per user, always global mode (monitors all tracked repos). PID file for enforcement.
- **Client fallback**: `jig ps` without `--watch` falls back to in-process behavior when daemon isn't running. `jig ps --watch` suggests `jig daemon start`.
- **Config reload**: Re-read from disk each poll cycle (every ~120s). No hot-reload signal.

## Phase 1 — Wire Protocol & Serializable Types

No behavior changes. Everything still compiles and works as before.

**1.1** Add `Serialize, Deserialize` to types that cross the wire: `MuxStatus`, `PrHealth`, `PrChecks`, `Branch` (verify), `RepoEntry` (verify).

**1.2** New file `daemon/ipc.rs` — define `Request` (GetStatus, Shutdown, Ping), `Response` (Status, Pong, Ok, Error), `WorkerSnapshot` (fully-serializable projection of `WorkerState`), `socket_path()`, `read_message`/`write_message` helpers.

**1.3** Implement `From<&WorkerState> for WorkerSnapshot` — maps all 30+ fields, converting `Url` → `String`, etc.

**1.4** Wire into `daemon/mod.rs` as `pub mod ipc;`. Unit tests for serialization round-trips.

## Phase 2 — Server-Side IPC Listener

**2.1** Add `Daemon::snapshot()` — packages actor state into `Response::Status` using the same reads `ps.rs` does today.

**2.2** Introduce `DaemonShared` — holds `Arc<MonitorActor>`, `Arc<TriageActor>`, `Arc<SpawnActor>`, `Arc<AtomicU64>` (poll_remaining), `Arc<Config>`. The tick loop updates atomics; the IPC listener reads without blocking ticks. This avoids `Arc<Mutex<Daemon>>`.

**2.3** IPC listener thread in `ipc.rs` — binds `UnixListener`, accepts connections with 500ms non-blocking poll, reads one `Request`, dispatches via `DaemonShared`, writes one `Response`, closes connection. Checks `quit` flag between accepts. Removes stale socket on startup (verify via PID file).

**2.4** PID file — `daemon_pid_path()` in `paths.rs`. Write on startup, remove on shutdown. Stale detection reuses existing `previous_run_crashed()` logic.

## Phase 3 — `jig daemon` Command

**3.1** Restructure `cli/commands/daemon.rs` into subcommands:

```
jig daemon start [--once]    # foreground, with IPC listener
jig daemon stop              # sends Shutdown via IPC
jig daemon status            # sends Ping, reports PID/uptime
jig daemon install           # registers OS service
jig daemon uninstall         # removes OS service
```

**3.2** `start` handler — check PID for existing instance, create `Context::from_global()`, start `Daemon`, spawn IPC listener thread with `DaemonShared`, install SIGINT/SIGTERM handlers, run tick loop, clean up socket+PID on exit.

**3.3** `stop` handler — connect to socket, send `Request::Shutdown`, report result.

**3.4** `status` handler — connect to socket, send `Request::Ping`, print PID + uptime or "not running".

## Phase 4 — Client-Side IPC in `jig ps`

**4.1** IPC client helper in `ipc.rs`: `connect_and_request(req) -> Result<Response, IpcError>`.

**4.2** Refactor `ps` one-shot — try IPC first, fall back to in-process if connection refused.

**4.3** Refactor `ps --watch` — poll `GetStatus` over IPC every interval. Keep the same TUI rendering. Fall back to in-process if daemon not running (preserving current behavior).

**4.4** `WorkerSnapshot::to_display_state() -> WorkerState` for the UI layer, which is heavily coupled to `WorkerState` through 10+ render functions.

## Phase 5 — OS Service Integration

**5.1** Add `service-manager = "0.7"` to workspace deps.

**5.2** `install` action — resolve `jig` binary path via `current_exe()`, create `ServiceConfig` with label `org.jig.daemon`, args `["daemon", "start"]`, user-level (`ServiceLevel::User`). Calls `manager.install()`. Handles launchd (macOS) and systemd (Linux) automatically.

**5.3** `uninstall` action — `manager.uninstall()`.

## Phase 6 — Testing

- Integration tests: `daemon start --once`, `daemon status` when not running, socket cleanup on crash, PID lifecycle.
- Unit tests: IPC round-trips, `WorkerSnapshot` conversion, socket path resolution.
- Backwards compat: `jig ps` output identical with and without daemon running.

## Risk Areas

| Risk | Mitigation |
|------|-----------|
| Missing serde derives on nested types | `WorkerSnapshot` indirection catches at boundary |
| Stale socket after SIGKILL | Startup checks: try connect → fail = stale → remove and rebind |
| Thread safety of shared state | Actors already use `Arc<A>` + `Mutex`; `poll_remaining` gets `AtomicU64` |
| `service-manager` user-level support | 0.7.x supports launchd user agents and `systemd --user` |

## New Dependencies

- `service-manager` — OS service integration
- None for IPC (`std::os::unix::net` is stdlib)

## Files Changed

| File | Change |
|------|--------|
| `daemon/ipc.rs` | New — protocol types, socket helpers, client/server |
| `daemon/mod.rs` | `snapshot()`, `DaemonShared`, IPC listener integration |
| `cli/commands/daemon.rs` | Restructure into subcommands |
| `cli/commands/ps.rs` | IPC client with in-process fallback |
| `context/paths.rs` | `socket_path()`, `daemon_pid_path()` |
| `worker/status.rs` | Serde derives on `MuxStatus` |
| `daemon/checks.rs` | Serde derives on `PrHealth`, `PrChecks` |
| `Cargo.toml` (workspace + jig-cli) | Add `service-manager` |
