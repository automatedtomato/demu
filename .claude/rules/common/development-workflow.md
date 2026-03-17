# Development Workflow

> This file covers the development process that happens before git operations.
> For commit conventions, see [git-workflow.md](./git-workflow.md).
> For the full GitHub process (issues, branches, PRs, review), see [github-workflow.md](./github-workflow.md).

The Feature Implementation Workflow describes the development pipeline: research, planning, TDD, code review, and then committing to git.

## Task tracking

Always maintain `tasks/todo.md` as the persistent record of work status across sessions.

- **At session start:** read `tasks/todo.md` to understand what is in progress and what is next.
- **During work:** update `tasks/todo.md` whenever a task starts, completes, or is blocked.
- **At session end:** ensure `tasks/todo.md` reflects the current state before stopping.

`tasks/todo.md` is the source of truth for ongoing work. It outlasts any single conversation and should be kept current even when the in-session TodoWrite tool is also in use.

Suggested format:

```markdown
## In progress
- [ ] #1 Cargo workspace scaffold — setting up module stubs

## Up next
- [ ] #2 Domain model types

## Done
- [x] milestone planning, design decisions (tasks/decisions/)
```

## Feature Implementation Workflow

0. **Research & Reuse** _(mandatory before any new implementation)_
   - **GitHub code search first:** Run `gh search repos` and `gh search code` to find existing implementations, templates, and patterns before writing anything new.
   - **Exa MCP for research:** Use `exa-web-search` MCP during the planning phase for broader research, data ingestion, and discovering prior art.
   - **Check package registries:** Search npm, PyPI, crates.io, and other registries before writing utility code. Prefer battle-tested libraries over hand-rolled solutions.
   - **Search for adaptable implementations:** Look for open-source projects that solve 80%+ of the problem and can be forked, ported, or wrapped.
   - Prefer adopting or porting a proven approach over writing net-new code when it meets the requirement.

1. **Plan First**
   - Use **planner** agent to create implementation plan
   - Generate planning docs before coding: PRD, architecture, system_design, tech_doc, task_list
   - Identify dependencies and risks
   - Break down into phases

2. **TDD Approach**
   - Use **tdd-guide** agent
   - Write tests first (RED)
   - Implement to pass tests (GREEN)
   - Refactor (IMPROVE)
   - Verify 80%+ coverage

3. **Code Review**
   - Use **code-reviewer** agent immediately after writing code
   - Address CRITICAL and HIGH issues
   - Fix MEDIUM issues when possible

4. **Commit & Push**
   - Follow conventional commits format — see [git-workflow.md](./git-workflow.md)
   - Follow issue-driven GitHub workflow — see [github-workflow.md](./github-workflow.md)
   - All work tied to a GitHub Issue; all merges through PRs targeting `develop`
