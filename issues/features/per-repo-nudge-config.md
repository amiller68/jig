# Per-repo nudge configuration in jig.toml

**Status:** Planned
**Priority:** High
**Labels:** auto

## Objective

Allow nudge behavior to be configured per-repo in `jig.toml` with per-nudge-type cooldown intervals, so different projects can tune thresholds, limits, and pacing for each nudge category independently.

## Context

Nudge settings currently live only in the global config (`~/.config/jig/config.toml` `[health]`):

```toml
[health]
silence_threshold_seconds = 300  # when a worker becomes "stalled"
max_nudges = 3                   # nudges before escalating to notification
```

Problems:
- **No per-repo overrides** — a fast TS project and a slow Rust project share the same thresholds
- **No per-type tuning** — review nudges, CI nudges, idle nudges, and stalled nudges all share `max_nudges` and have no independent cooldown
- **PR nudges burst** — review/CI/conflict nudges fire every tick (~2s) with no cooldown, dumping all 3 nudges in 6 seconds (see `issues/bugs/pr-nudge-burst.md`)
- **Not visible** — `jig config show` doesn't display any of these values

## Design

### Config shape

```toml
# jig.toml
[health]
silence_threshold_seconds = 600   # override global stall detection
max_nudges = 5                    # override global default limit

# Per-nudge-type overrides (all optional)
[health.nudge.idle]
max = 3                           # max idle nudges before escalation
cooldown_seconds = 300            # minimum seconds between idle nudges

[health.nudge.stalled]
max = 3
cooldown_seconds = 600            # stalled workers get more time

[health.nudge.ci]
max = 3
cooldown_seconds = 300

[health.nudge.review]
max = 3
cooldown_seconds = 300

[health.nudge.conflict]
max = 2
cooldown_seconds = 300

[health.nudge.bad_commits]
max = 2
cooldown_seconds = 300
```

Resolution order: `jig.toml [health.nudge.<type>]` > `jig.toml [health]` > global config > defaults.

When `[health.nudge.<type>].max` is not set, it falls back to `[health].max_nudges`. When `cooldown_seconds` is not set, it defaults to `silence_threshold_seconds` (the stall interval), which is a sensible default — "give the agent as long to respond as we'd wait before calling it stalled."

## Implementation

### 1. Add config structs in `crates/jig-core/src/config.rs`

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoHealthConfig {
    pub silence_threshold_seconds: Option<u64>,
    pub max_nudges: Option<u32>,
    #[serde(default)]
    pub nudge: NudgeTypeConfigs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NudgeTypeConfigs {
    pub idle: Option<NudgeTypeConfig>,
    pub stalled: Option<NudgeTypeConfig>,
    pub ci: Option<NudgeTypeConfig>,
    pub review: Option<NudgeTypeConfig>,
    pub conflict: Option<NudgeTypeConfig>,
    pub bad_commits: Option<NudgeTypeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeTypeConfig {
    pub max: Option<u32>,
    pub cooldown_seconds: Option<u64>,
}
```

Add a resolver that takes a `NudgeType` and returns `(max, cooldown)`:

```rust
impl RepoHealthConfig {
    pub fn resolve_for_nudge_type(
        &self,
        nudge_type: &NudgeType,
        global: &HealthConfig,
    ) -> (u32, u64) {
        let base_max = self.max_nudges.unwrap_or(global.max_nudges);
        let base_cooldown = self.silence_threshold_seconds
            .unwrap_or(global.silence_threshold_seconds);

        let type_config = match nudge_type {
            NudgeType::Idle => &self.nudge.idle,
            NudgeType::Stuck => &self.nudge.stalled,
            NudgeType::Ci => &self.nudge.ci,
            NudgeType::Review => &self.nudge.review,
            NudgeType::Conflict => &self.nudge.conflict,
            NudgeType::BadCommits => &self.nudge.bad_commits,
        };

        let max = type_config.as_ref()
            .and_then(|c| c.max)
            .unwrap_or(base_max);
        let cooldown = type_config.as_ref()
            .and_then(|c| c.cooldown_seconds)
            .unwrap_or(base_cooldown);

        (max, cooldown)
    }
}
```

### 2. Thread resolved config through nudge dispatch

Update the PR nudge path (`crates/jig-core/src/daemon/mod.rs:536-565`) and the idle/stalled nudge path (`crates/jig-core/src/nudge.rs:classify_nudge`) to:
1. Resolve `(max, cooldown)` for the specific nudge type
2. Check `count >= max` (already exists)
3. Check `last_nudge_timestamp + cooldown > now` (new — requires reading the last Nudge event timestamp for this type from the event log)

### 3. Track last nudge timestamp per type

The event log already records Nudge events with timestamps and the nudge type in `event.data`. Add a `last_nudge_at: HashMap<String, i64>` field to `WorkerState` (populated by the reducer) so the cooldown check doesn't need to scan the full event log.

### 4. Show in `jig config show`

Display effective nudge config per type in the config output.

## Files

- `crates/jig-core/src/config.rs` — `RepoHealthConfig`, `NudgeTypeConfigs`, `NudgeTypeConfig`, resolver
- `crates/jig-core/src/events/reducer.rs` — Track `last_nudge_at` per type in `WorkerState`
- `crates/jig-core/src/events/derive.rs` — Accept resolved `silence_threshold_seconds`
- `crates/jig-core/src/nudge.rs` — Accept resolved per-type max and cooldown
- `crates/jig-core/src/daemon/mod.rs` — Resolve per-repo + per-type config, apply cooldown to PR nudges
- `crates/jig-core/src/daemon/pr.rs` — Same for blocking path
- `crates/jig-cli/src/commands/config.rs` — Render nudge config

## Acceptance Criteria

- [ ] `jig.toml` supports `[health]` with global overrides and `[health.nudge.<type>]` with per-type overrides
- [ ] Each nudge type resolves its own `max` and `cooldown_seconds` independently
- [ ] Cooldown prevents re-nudging the same type within `cooldown_seconds`
- [ ] Stall detection uses per-repo `silence_threshold_seconds`
- [ ] `jig config show` displays effective nudge config
- [ ] Existing behavior unchanged when `[health]` is not present in `jig.toml`
- [ ] This also fixes the PR nudge burst bug (`issues/bugs/pr-nudge-burst.md`)

## Verification

```bash
# Configure per-type nudge intervals
cat >> jig.toml <<'EOF'
[health]
silence_threshold_seconds = 120

[health.nudge.review]
max = 5
cooldown_seconds = 180

[health.nudge.ci]
max = 3
cooldown_seconds = 60
EOF

jig config show
# Should show per-type resolved values

# Trigger a PR with review comments
# Verify review nudge 1 fires immediately
# Verify review nudge 2 fires after 180s, not 2s
```
