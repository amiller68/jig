---
layout: home
title: Home
nav_order: 1
---

# welcome to jig!

**Multiply yourself across parallel agent sessions.**

jig is a git worktree manager for orchestrating Agentic Coding Assistants (ACAs) in parallel. It helps you scale your skills across multiple AI coding sessions, getting more done while spending your time on what matters: deciding *what* to build rather than the minutiae of *how*.

*NOTE*: jig is designed to work with all sorts of ACAs, but I say that with the following caveats:
- it makes the most sense to use with minimal TUIs. Jig is designed to help you stay in there terminal focused on getting work done.
- jig is extensible to any ACA with user-defined skills and commands (which is pretty much all of them). That being said, I am a very heavy Claude Code user, and I prioritize feature development and testing for that. If I assume something that does not apply to ACAs in general, I apologize, and please feel free to reach out!

## What jig does

jig doesn't do much—because it doesn't need to. ACAs are already good at following instructions and using tools. jig provides:

- **Worktree tooling** — Create, manage, and clean up isolated git worktrees where agents work
- **Worker spawning** — Launch agent sessions in tmux with task context
- **Scaffolding** — Bootstrap repos with documentation structure, issue tracking, and agent-friendly conventions
- **An opinionated workflow** — ticket → worktree → agent work → review → merge

## What jig is not

jig is not a vibe coding silver bullet. It won't make bad prompts good or replace engineering judgment. It's a force multiplier for developers who:

- Write well-scoped tickets
- Maintain helpful, organized documentation
- Enforce motivated coding standards
- Want to supervise multiple agents instead of waiting on one

## Quick start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/amiller68/jig/main/install.sh | bash

# Add shell integration (bash or zsh)
eval "$(jig shell-init zsh)"

# Initialize jig in your repo
jig init

# Create a worktree and spawn an agent
jig spawn feature-auth --context "Implement JWT authentication"

# Check on your workers
jig list

# Review and merge when done
jig review feature-auth
jig merge feature-auth
```

## Learn more

- [Background](/background) — Why parallel agents, and what jig means for engineers
- [Core Concepts](/concepts) — Worktrees, documentation, issues, and quality
- [Getting Started](/getting-started) — Installation and first steps

---

<p style="text-align: center; opacity: 0.7; margin-top: 3rem;">
  <a href="https://github.com/amiller68/jig">GitHub</a>
</p>
