---
layout: page
title: Background
nav_order: 2
---

# Background

## ACAs vs Engineers

So much discourse lately has focused on whether Agentic Coding Assistants (ACAs) spell doom for software engineers. My opinion: such fears are overblown, but they should be balanced with the new reality these tools present.

Consider a single ACA working on a fullstack feature: writing a database migration, implementing a library method, exposing an API endpoint, and adding a UI component. ACAs excel at straightforward tasks like this. With a good description and a planning step, they can often complete such work with minimal guidance.

**But ACAs have limitations:**

- They're slow. They gather context, make tool calls, and generate tokens—all of which takes time.
- They're inconsistent. They may hallucinate requirements, write verbose code, or invent bizarre workarounds.
- They need supervision. Getting them out of loops, over hurdles, and past poor product decisions requires human judgment.

Think of each fresh ACA instance as a junior-to-mid engineer who just joined your team without context on your codebase. They'll spend time exploring before they can be productive.

Now consider a mid-to-senior engineer familiar with the codebase. They don't need detailed instructions—they probably wrote the ticket themselves. They know the patterns, the tools, the gotchas. With professional focus, they'd likely outperform a single lightly-supervised agent in speed and first-attempt accuracy.

**So why use ACAs at all?**

Because humans are also fallible. Focus wavers. Engineers have strengths in some areas and struggle in others. You hit opaque errors, bundling issues, unfamiliar frameworks. ACAs perform consistently, can hold more context for debugging, and adapt expertise to whatever the task requires.

## The third paradigm

There are three ways to work with ACAs:

1. **No agents** — The senior engineer working alone, in sequence. Fast and accurate, but doesn't scale.

2. **Passive tab monkey** — Waiting for one agent to finish, hitting accept, waiting again. You become a very passive participant, spending most of your time *waiting*.

3. **Parallel orchestration** — Running 2, 3, 5 agents simultaneously across isolated worktrees. You shuttle rote coding to agents while supervising, blessing, and steering their work. You amortize downtime across parallel sessions.

The third paradigm is what jig enables. You're not replaced—you become:

- A **product owner** defining objectives and breaking down work
- A **theoretician** on code composability, designing patterns agents can follow
- A **manager** of a team composed primarily of ACAs

## Why worktrees?

Back in the day, I used worktrees sparingly—for messy experiments or refactors with high failure likelihood. The utility was isolating mess from my main development environment.

Recently, worktrees have become invaluable for containing the messiness not of sleepy human coders—but coding agents.

Worktrees let you:

- **Isolate agent work** — Each agent operates in its own directory with its own branch
- **Replicate environments** — Create sandboxes where agents iterate without stepping on each other
- **Parallelize work** — Run multiple agents on different tasks simultaneously

This is the prime innovation: using worktrees to parallelize work across agents.

## The engineer's identity crisis

Is this the end of software engineering? I'm biased—I never saw myself writing code full-time 10 years into my career. I'm impatient and hate waiting for things to get done. And honestly? It's more worth my time to iterate on product than implementation details.

That said: balancing quality, modularity, performance, and extensibility are skills needed to iterate quickly. It's *worth* taking a day to generify an interface if you'll extend it for more use cases. This makes your code:

- Easier to maintain
- Easier to document
- Easier for agents to extend
- While keeping review manageable for humans

The future isn't agents replacing engineers. It's engineers who leverage agents replacing those who don't.
