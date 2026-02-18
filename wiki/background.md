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

## The jig way

jig is opinionated about how teams should work with agents at scale.

### Engineers take on product responsibilities

Engineers spend more time:

- Defining issues and writing detailed tickets
- Scoping work into parallelizable units
- Thinking through requirements before spawning agents
- Reviewing and approving agent-generated code

The role shifts from "person who writes code" to "person who defines what code should be written and ensures it's correct."

### Context engineering is a first-class concern

Documentation and context maintenance are emphasized to:

- Keep code quality consistent across agent sessions
- Provide contextual shortcuts that reduce agent exploration time
- Encode patterns and conventions agents can follow
- Make onboarding (for humans and agents) frictionless

Well-maintained `CLAUDE.md`, `PATTERNS.md`, and issue templates pay compounding dividends.

### Teams spend more time on

- Thinking clearly about technical requirements and best approaches
- Defining shared patterns, documentation, and objectives
- Responding to users, designing new features
- Reviewing AI-generated code
- Breaking down large initiatives into well-scoped tickets

### Teams spend less time on

- Writing code directly
- Debugging low-level issues
- Context-switching between implementation details
- Waiting for single-threaded agent sessions

The goal: multiply your judgment and taste across parallel agent sessions, shipping more while maintaining quality.
