---
layout: page
title: Background
nav_order: 2
---

# Background

## ACAs vs Engineers

So much discourse lately (writing in February 2026) has focused on whether Agentic Coding Assistants (ACAs) spell doom for software companies and professional engineers. My opinion: such fears are overblown, but they should be balanced with the new reality these tools present.

To understand how coders should position themselves in relation to AI, consider a single ACA working on the same issue as a senior engineer. Say you're working on a web app and the task involves implementing a new fullstack feature: writing a database migration, implementing a library method, exposing functionality through a new API endpoint, and modifying an existing page with a new UI component.

### Where ACAs excel

ACAs are remarkably good at straightforward fullstack issues like this. Depending on the size and intelligibility of your codebase, you may be able to prompt one to complete such a task with only a couple descriptive sentences and a planning step.

If you're diligent about writing detailed issue descriptions—calling out which files need editing, common workflows for your development database, reusable design patterns for the UI—you can cut down on exploration time and token usage, getting to a workable draft more quickly.

### Where ACAs struggle

That said, ACAs are not speedy, nor are they *always* stellar at writing quality, maintainable code.

Think of each fresh ACA instance as a junior-to-mid level engineer who just joined your team without context on your codebase. When working on a new issue, they'll spend appreciable time building context. They make multiple targeted tool calls to land edits and test changes. Depending on how well-scoped the task is, they may:

- Hallucinate product requirements
- Write needlessly verbose or duplicated code
- Invent bizarre workarounds to intermittent failures

(I should really maintain a blooper reel of AI mess-ups. Anyone who works extensively with ACAs knows exactly what I mean.)

The experience is impressive, but still requires non-trivial oversight from a supervising engineer to fill gaps in reasoning, bless decisions, and course-correct errors.

### The human engineer's edge

Now consider a mid-to-senior engineer familiar with the codebase. They don't need detailed instructions—they probably wrote the ticket themselves. They don't need to explore the repository or gather context. They've been active in defining the repo's standards and developing the tooling and patterns for database work, library code, API endpoints, and UI components.

Working directly in their development environment, with their editor of choice and professional focus, the human engineer would likely outperform a lone ACA in speed and first-attempt accuracy.

But humans are fallible too. Focus wavers throughout a day, week, or month. Engineers may have proficiency in backend code but struggle with frontend. You hit opaque errors from upstream libraries, hard-to-decipher bundling issues. These are drains on productivity where ACAs can help fill gaps—they perform consistently, hold more context for debugging opaque issues, and adapt expertise to whatever the task requires.

### The verdict

ACAs are great. They're not perfect. There's still a place for human engineers.

But if you're only using an LLM assistant in sequence—prompting, waiting, accepting, prompting again—you become a passive tab monkey, spending most of your time *waiting*. That's not a good use of anyone's time.

## Two paths forward

This leaves us with two approaches for increasing proficiency with AI in development:

### 1. Integrate agents into experimental work

Humans are the drivers of development. AI assists by writing boilerplate, producing first drafts, and debugging complex issues. Judgment and oversight remain key. This works well for:

- Exploratory or experimental work
- Novel problem-solving
- Architecture decisions
- Anything requiring deep context and taste

### 2. Optimize the pipeline for straightforward work

Humans guide work by writing well-scoped tickets and maintaining documentation. Agents execute with specifically-engineered context. We build tooling that makes this pipeline frictionless. This works well for:

- Routine feature implementation
- Bug fixes with clear reproduction steps
- Test coverage expansion
- Documentation updates

**jig is solving for the second path.**

## The third paradigm

There are three ways to work with ACAs:

1. **No agents** — The senior engineer working alone, in sequence. Fast and accurate, but doesn't scale.

2. **Passive tab monkey** — Waiting for one agent to finish, hitting accept, waiting again. You spend most of your time *waiting*.

3. **Parallel orchestration** — Running 2, 3, 5 agents simultaneously across isolated worktrees. You shuttle rote coding to agents while supervising, blessing, and steering their work. You amortize downtime across parallel sessions.

The third paradigm is what jig enables. You're not replaced—you become:

- A **product owner** defining objectives and breaking down work
- A **theoretician** on code composability, designing patterns agents can follow
- A **manager** of a team composed primarily of ACAs

## Why worktrees?

Back in the day, I used worktrees sparingly—for messy experiments or refactors likely to fail. The utility was isolating mess from my main development environment, so stray files and test data couldn't be picked up by an unscrupulous `git add -A`.

Recently, worktrees have become invaluable for containing the messiness not of sleepy human coders—but coding agents.

Worktrees let you:

- **Isolate agent work** — Each agent operates in its own directory with its own branch
- **Replicate environments** — Create sandboxes where agents iterate without stepping on each other
- **Parallelize work** — Run multiple agents on different tasks simultaneously

This is the prime innovation: using worktrees to parallelize work across agents.

## The engineer's identity crisis

Is this the end of software engineering? I'm biased—I never saw myself writing code full-time 10 years into my career. I'm impatient and hate waiting for things to get done. Honestly? It's more worth my time to iterate on product than implementation details.

That said: balancing quality, modularity, performance, and extensibility are skills needed to iterate quickly. It's *worth* taking a day to generify an interface if you'll extend it for more use cases. This makes your code:

- Easier to maintain
- Easier to document
- Easier for agents to extend
- While keeping review manageable for humans

The future isn't agents replacing engineers. It's engineers who leverage agents replacing those who don't.
