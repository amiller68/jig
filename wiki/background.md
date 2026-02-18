---
layout: page
title: Background
nav_order: 2
---

# Background

## ACAs vs Engineers

So much discourse has focused on whether Agentic Coding Assistants (ACAs) spell doom for software engineers. My opinion: such fears are overblown, but they should be balanced with the new reality these tools present.

Consider a single ACA working on a fullstack feature: database migration, library method, API endpoint, UI component. ACAs excel at straightforward tasks like this. With a good description and a planning step, they can often complete such work with minimal guidance.

If you write detailed issue descriptions—calling out which files need editing, common workflows, reusable patterns—you cut down on exploration time and get to a workable draft faster.

### Where ACAs struggle

ACAs are not speedy, nor always stellar at writing quality code.

Think of each fresh ACA instance as a junior engineer who just joined your team without context. They'll spend time building context, making multiple tool calls to land edits. Depending on scope, they may:

- Hallucinate requirements
- Write verbose or duplicated code
- Invent bizarre workarounds to intermittent failures

Impressive, but still requires oversight to fill gaps in reasoning, bless decisions, and course-correct errors.

### The human engineer's edge

A mid-to-senior engineer familiar with the codebase doesn't need detailed instructions—they probably wrote the ticket themselves. They know the patterns, tools, and gotchas. With professional focus, they'd outperform a lone ACA in speed and accuracy.

But humans are fallible too. Focus wavers. Engineers have strengths in some areas and struggle in others. You hit opaque errors, bundling issues, unfamiliar frameworks. ACAs help fill these gaps—they perform consistently, hold more context for debugging, and adapt expertise to the task.

### The verdict

ACAs are great. Not perfect. There's still a place for human engineers.

But if you're only using an LLM assistant in sequence—prompting, waiting, accepting, prompting again—you become a passive tab monkey. That's not a good use of anyone's time.

## Two paths forward

Two approaches for increasing proficiency with AI in development:

### 1. Integrate agents into experimental work

Humans drive development. AI assists with boilerplate, first drafts, and debugging. Judgment and oversight remain key. Good for:

- Exploratory work
- Novel problem-solving
- Architecture decisions

### 2. Optimize the pipeline for straightforward work

Humans guide work via well-scoped tickets and documentation. Agents execute with engineered context. We build tooling that makes this frictionless. Good for:

- Routine feature implementation
- Bug fixes with clear repro steps
- Test coverage
- Documentation

**jig solves for the second path.**

## The third paradigm

Three ways to work with ACAs:

1. **No agents** — Senior engineer working alone. Fast and accurate, but doesn't scale.

2. **Passive tab monkey** — Waiting for one agent to finish, hitting accept, waiting again. Most of your time is spent *waiting*.

3. **Parallel orchestration** — Running multiple agents across isolated worktrees. You shuttle rote coding to agents while supervising and steering. Downtime is amortized across parallel sessions.

The third paradigm is what jig enables. You become:

- A **product owner** defining objectives and breaking down work
- A **theoretician** on code composability, designing patterns agents can follow
- A **manager** of a team composed primarily of ACAs

## Why worktrees?

Worktrees used to be for messy experiments or risky refactors—isolating mess from your main environment.

Now they're invaluable for containing the messiness of coding agents.

Worktrees let you:

- **Isolate agent work** — Each agent operates in its own directory and branch
- **Replicate environments** — Sandboxes where agents iterate without conflicts
- **Parallelize work** — Multiple agents on different tasks simultaneously

This is the prime innovation: using worktrees to parallelize work across agents.

## The engineer's identity crisis

Is this the end of software engineering? Honestly, it's more worth my time to iterate on product than implementation details.

That said: balancing quality, modularity, and extensibility are skills needed to iterate quickly. It's worth taking a day to generify an interface you'll extend later. This makes code:

- Easier to maintain
- Easier to document
- Easier for agents to extend

The future isn't agents replacing engineers. It's engineers who leverage agents replacing those who don't.
