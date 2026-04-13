---
title: Triage
slug: triage
date: 2026-04-13
releases: ["v1.12"]
---

When a new issue lands in your Linear board, someone has to investigate it before real work begins: find the relevant code, assess scope, figure out an approach. Triage automates that step. The daemon discovers issues in Linear's Triage state, spawns a read-only AI agent to investigate the codebase, and writes findings back to the issue. The issue moves to Backlog, and a human decides what happens next.

No worktree, no tmux session, no code changes. Triage agents investigate and report — that's it.

## The flow

```text
Issue created in Linear with status "Triage"
  → Daemon polls and discovers it
    → Spawns a subprocess agent (read-only)
      → Agent reads the codebase, identifies affected code
        → Agent updates the issue description with findings
          → Agent transitions the issue to Backlog
            → Human reviews enriched issue and decides next steps
```

### Step by step

1. **Issue enters Triage** — you (or your team) create an issue in Linear and set its status to Triage. This is the signal that investigation is needed before the work is ready to plan.

2. **Daemon discovers it** — on the next tick, the daemon's issue poll picks up all issues with Triage status. It filters out any that are already being triaged (tracked in memory to prevent duplicate spawns).

3. **Subprocess spawns** — the daemon launches a one-shot Claude Code subprocess. No tmux window, no worktree. The agent receives the issue title, body, and labels as a rendered prompt piped to stdin.

4. **Agent investigates** — using its read-only tool set, the agent explores the codebase: finding relevant files, tracing code paths, assessing scope. It produces a structured analysis.

5. **Agent updates Linear** — when done, the agent appends its findings to the issue description and transitions the issue from Triage to Backlog:

   ```bash
   jig issues update JIG-42 --body "investigation findings..."
   jig issues status JIG-42 backlog
   ```

6. **Subprocess exits** — the daemon picks up the result on the next tick. If the exit code was 0, the triage entry is cleared. If it failed, a NeedsIntervention notification fires.

7. **Human reviews** — the issue now sits in Backlog with a full investigation attached. You can promote it to Planned (the daemon will auto-spawn a worker), tweak the scope, assign it to a human, or reject it.

## Configuration

Enable triage per-repo in `jig.toml`:

```toml
[triage]
enabled = true
model = "sonnet"
timeout_seconds = 600
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Whether the daemon auto-triages issues for this repo |
| `model` | `"sonnet"` | Which model the triage agent uses. Sonnet is the default — fast enough for investigation, no need for opus |
| `timeout_seconds` | `600` | How long before a triage subprocess is considered stuck (10 minutes) |

Triage respects the same provider filters as auto-spawn. If your Linear config scopes to a specific team, project, or label set, triage issues must match those filters to be visible.

## What the agent can do

Triage agents run with a restricted tool set — read-only access plus the jig CLI and Linear MCP:

| Tool | Purpose |
|------|---------|
| `Read` | Read files in the repo |
| `Glob` | Find files by pattern |
| `Grep` | Search file contents |
| `Bash(jig *)` | Run jig CLI commands (issues update, issues status) |
| `mcp__linear*` | Query Linear for additional context |

No `Edit`, no `Write`, no general `Bash`. The agent cannot modify code — it can only read the codebase and write back to Linear.

### What the agent produces

The triage prompt instructs the agent to produce a structured analysis:

```markdown
### Investigation
What the agent found about affected code, scope, and approach.

### Affected Files
- `path/to/file.rs` — reason this file is relevant

### Proposed Approach
1. Step one
2. Step two

### Complexity
Small | Medium | Large

### Suggested Priority
Urgent | High | Medium | Low

### Risks
- Any risks or concerns
```

This gets appended to the Linear issue body, so when you open the issue you see both the original description and the triage findings in one place.

## How the daemon tracks triages

The daemon maintains a `TriageTracker` — an in-memory registry of in-flight triage subprocesses. Each entry records the issue ID, a worker name (e.g., `triage-jig-42`), the repo, and a spawn timestamp.

On every tick, the daemon:

1. **Drains results** — collects completion/failure reports from triage subprocesses that finished since last tick. Clears their tracker entries.
2. **Polls for Triage issues** — asks the issue provider for all issues with Triage status.
3. **Deduplicates** — filters out any issue that already has an active triage entry.
4. **Detects stuck workers** — checks each active entry's age against the repo's `timeout_seconds`. If exceeded, clears the entry and emits NeedsIntervention.
5. **Dispatches new triages** — registers new entries and sends them to the triage actor for subprocess execution.

The tracker persists to `~/.config/jig/state/triages.json` so it can recover state across daemon restarts.

## Monitoring triages

`jig ps` shows in-flight triages alongside regular workers:

```bash
jig ps
```

Each triage entry displays the issue ID, model, elapsed time, and repo name. You can also list issues by status to see what's queued or completed:

```bash
jig issues list --status triage    # waiting to be triaged
jig issues list --status backlog   # triaged, awaiting human review
```

## How triage differs from spawn

| | Triage | Spawn |
|---|--------|-------|
| **Trigger** | Issue in Triage status | Issue in Planned status with auto-spawn label |
| **Execution** | One-shot subprocess | Persistent tmux session |
| **Worktree** | None | Dedicated git worktree |
| **Tools** | Read-only (Read, Glob, Grep, jig CLI, Linear MCP) | Full access (Edit, Write, Bash, all MCP) |
| **Output** | Updates issue description, transitions to Backlog | Commits code, opens draft PR |
| **Duration** | Minutes (default timeout: 10 min) | Hours to days |
| **Model** | Sonnet (configurable) | Configurable per-repo |
| **Session** | Ephemeral, no persistence | Persistent, resumable |

Triage is the lightweight investigation step that feeds into the heavier spawn pipeline. An issue flows: Triage → Backlog → (human promotes) → Planned → Spawn.

## Failure and recovery

| Scenario | What happens |
|----------|-------------|
| Agent exits successfully but doesn't update the issue | Daemon sees the issue still in Triage on the next poll. Emits NeedsIntervention |
| Subprocess crashes (non-zero exit) | Triage actor reports the failure. NeedsIntervention notification fires |
| Subprocess hangs past `timeout_seconds` | Daemon detects the stuck entry, clears it, emits NeedsIntervention |
| Duplicate spawn attempt | TriageTracker filters it out — only one triage per issue at a time |

There is no automatic retry. A failed triage requires human intervention — either re-setting the issue to Triage status in Linear or investigating what went wrong.

## Customizing the triage prompt

The triage prompt is a built-in Handlebars template (`triage-prompt`). It receives the issue ID, title, body, and labels as context variables. Like other jig templates, you can override it per-repo by placing a custom template in your repo's `.jig/templates/` directory.

The default prompt instructs the agent to:
1. Identify affected code
2. Assess scope (small fix, medium refactor, large feature)
3. Propose an approach
4. Flag risks and dependencies
5. Suggest a priority level
