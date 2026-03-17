# AGENTS.md

This file tells how to work in this repository.

## Mission

Build `demu`, a **fast, non-destructive Docker/Compose preview shell**.

The product should help users answer questions like:

- What files would exist after this Dockerfile is interpreted?
- What is the effective working directory?
- What environment variables would be visible?
- Which packages appear to be installed?
- Which files came from `COPY`, `RUN`, mounts, or previous stages?

## Critical truth

`demu` is **not a container runtime**.

Do not optimize for runtime correctness.
Optimize for **fast, useful, explainable previews**.

## Priority order

When tradeoffs happen, follow this order:

1. Clear user mental model
2. Fast iteration speed
3. Structural fidelity of filesystem/env/stage/mount state
4. Good explanations for simulated behavior
5. Broad Docker feature coverage
6. Pretty UI
7. Perfect emulation

## Hard constraints

- Do not require Docker daemon for MVP
- Do not launch real containers for MVP
- Do not attempt full shell emulation
- Do not attempt network access for `RUN`
- Do not attempt real dependency solving
- Do not silently fake behavior without surfacing that it is simulated

## Product boundary

Allowed to simulate:

- Filesystem mutations from a safe subset of shell commands
- Installed package registries
- `which`, `pip list`, `apt list --installed`, and similar common inspection commands
- Compose service views as merged configuration previews

Not allowed to promise:

- Real package resolution
- Binary executability
- OS-specific runtime compatibility
- Bit-for-bit Docker parity

## Implementation style

- Use Rust
- Prefer small, typed modules
- Keep parser, engine, and REPL separate
- Make internal state easy to inspect in tests
- Prefer explicit enums over stringly-typed branching
- Prefer deterministic behavior

## Claude configuration catalog

**Read this section at the start of every session and whenever you are unsure how to proceed.**

The `.claude/` directory contains rules, agents, commands, and skills that govern how to work in this repository. Consult the relevant resource before making decisions.

### Rules

Standards and checklists that apply to all code in this repo.

| File | Purpose |
|------|---------|
| [.claude/rules/README.md](.claude/rules/README.md) | Structure, installation, and priority rules |
| [.claude/rules/common/coding-style.md](.claude/rules/common/coding-style.md) | Universal coding style principles |
| [.claude/rules/common/testing.md](.claude/rules/common/testing.md) | Universal testing standards |
| [.claude/rules/common/patterns.md](.claude/rules/common/patterns.md) | Universal design patterns |
| [.claude/rules/common/security.md](.claude/rules/common/security.md) | Universal security rules |
| [.claude/rules/common/performance.md](.claude/rules/common/performance.md) | Universal performance guidelines |
| [.claude/rules/common/hooks.md](.claude/rules/common/hooks.md) | PostToolUse hook conventions |
| [.claude/rules/common/agents.md](.claude/rules/common/agents.md) | When and how to use sub-agents |
| [.claude/rules/common/git-workflow.md](.claude/rules/common/git-workflow.md) | Git commit and branch conventions |
| [.claude/rules/common/github-workflow.md](.claude/rules/common/github-workflow.md) | PR and review conventions |
| [.claude/rules/common/development-workflow.md](.claude/rules/common/development-workflow.md) | Day-to-day development workflow |
| [.claude/rules/rust/coding-style.md](.claude/rules/rust/coding-style.md) | Rust: rustfmt, clippy, error handling, enums |
| [.claude/rules/rust/testing.md](.claude/rules/rust/testing.md) | Rust: test organization, fixtures, coverage |
| [.claude/rules/rust/patterns.md](.claude/rules/rust/patterns.md) | Rust: Builder, Newtype, Typestate, traits |
| [.claude/rules/rust/hooks.md](.claude/rules/rust/hooks.md) | Rust: cargo fmt/clippy/check hooks |
| [.claude/rules/rust/security.md](.claude/rules/rust/security.md) | Rust: unsafe policy, cargo audit, no-unwrap |

### Agents

Specialized sub-agents to delegate work to. Invoke via the Agent tool.

| Agent | When to use |
|-------|------------|
| [architect](.claude/agents/architect.md) | System design, architectural decisions, planning new modules |
| [planner](.claude/agents/planner.md) | Step-by-step implementation plans for complex features |
| [tdd-guide](.claude/agents/tdd-guide.md) | Write tests first, enforce 80%+ coverage |
| [code-reviewer](.claude/agents/code-reviewer.md) | Review code for quality, security, and maintainability |
| [security-reviewer](.claude/agents/security-reviewer.md) | Detect vulnerabilities in user-input handling, auth, APIs |
| [qa-engineer](.claude/agents/qa-engineer.md) | Verify test quality and behavioral completeness |
| [build-error-resolver](.claude/agents/build-error-resolver.md) | Fix build/type errors with minimal diffs |
| [refactor-cleaner](.claude/agents/refactor-cleaner.md) | Remove dead code and consolidate duplicates |
| [doc-updater](.claude/agents/doc-updater.md) | Update documentation and codemaps |
| [harness-optimizer](.claude/agents/harness-optimizer.md) | Improve agent harness configuration |
| [loop-operator](.claude/agents/loop-operator.md) | Monitor and intervene in autonomous agent loops |

### Commands

Slash commands for common workflows. Invoke with `/command-name`.

| Command | Purpose |
|---------|---------|
| [/plan](.claude/commands/plan.md) | Restate requirements, assess risks, produce an implementation plan — waits for confirmation before touching code |
| [/tdd](.claude/commands/tdd.md) | Scaffold interfaces, generate tests first, implement minimal passing code |
| [/orchestrate](.claude/commands/orchestrate.md) | Chain planner → tdd-guide → code-reviewer → security-reviewer for complex tasks |

### Skills

Deep reference material for specific tasks. Loaded on demand.

| Skill | Purpose |
|-------|---------|
| [claude-api](.claude/skills/claude-api/SKILL.md) | Building apps with the Claude API / Anthropic SDK |
| [tdd-workflow](.claude/skills/tdd-workflow/SKILLS.md) | TDD methodology and patterns |
| [strategic-compact](.claude/skills/strategic-compact/SKILLS.md) | Strategic planning and compaction |
| [compare-and-contrast](.claude/skills/compare-and-contrast/SKILLS.md) | Structured option comparison before deciding |
| [continuous-learning](.claude/skills/continuous-learning/SKILLS.md) | Session observation and instinct capture |
| [pre-merge-review](.claude/skills/pre-merge-review/SKILLS.md) | Pre-merge quality checklist |
| [agent-harness-construction](.claude/skills/agent-harness-construction/SKILLS.md) | Designing agent action spaces and tool definitions |
| [skill-creator](.claude/skills/skill-creator/SKILL.md) | Creating and evaluating new skills |

---

## Required reading order for agents

Before changing code, read in this order:

1. `README.md`
2. `docs/01-product.md`
3. `docs/02-architecture.md`
4. `docs/03-cli-and-repl.md`
5. `docs/04-dockerfile-semantics.md`
6. `docs/05-run-simulation.md`
7. `docs/08-test-strategy.md`

If implementing Compose features, also read:

8. `docs/06-compose-plan.md`

## Rules for changes

### When adding a feature

- Update the relevant design doc first if behavior changes
- Then update code
- Then add or update tests
- Then update README only if user-facing behavior changed

### When behavior is ambiguous

- Prefer a simpler, explainable model
- Document the approximation
- Expose the approximation in `:history`, `:layers`, `:installed`, or warnings

### When `RUN` is involved

- Never silently execute arbitrary host shell commands
- Only simulate commands from the approved subset
- Mark unsupported commands as skipped or unmodeled
- Preserve command text in history

### When package installs are involved

- Record packages in an internal registry
- Make them visible via preview commands
- Do not pretend to download real packages
- Do not model dependency trees unless explicitly added later

## Coding guidelines

- Keep functions short and composable
- Minimize global mutable state
- Use modules for distinct concerns:
  - `parser/*`
  - `model/*`
  - `engine/*`
  - `repl/*`
  - `explain/*`
- Add integration fixtures for each supported Dockerfile behavior

## UX guidelines

- Default experience is REPL-first
- Output should be compact and terminal-friendly
- The shell should feel plain, close to a normal Linux shell
- Custom commands should use `:` prefix
- Errors should explain whether something is unsupported, skipped, or simulated

## MVP acceptance bar

The MVP is acceptable only if a user can:

1. Run `demu -f Dockerfile`
2. Inspect files with `ls`, `cat`, and `find`
3. See environment variables with `env`
4. Understand what happened in `RUN`
5. See simulated package installs
6. Ask where a file came from with `:explain`

If one of these is missing, MVP is incomplete.
