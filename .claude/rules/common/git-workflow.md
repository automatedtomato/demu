# Git Workflow

> This file covers commit conventions and local git practices.
> For the full GitHub-based development process (issues, branches, PRs, review),
> see [github-workflow.md](./github-workflow.md).
> For the pre-commit development process (planning, TDD, code review),
> see [development-workflow.md](./development-workflow.md).

## Commit Message Format
```
<type>: <description>

<optional body>
```

Types: feat, fix, refactor, docs, test, chore, perf, ci

Note: Attribution disabled globally via ~/.claude/settings.json.

## Commit Practices

- Commit messages should explain **why**, not just what.
- Keep commits focused — one logical change per commit.
- Reference the GitHub Issue number in the commit body when relevant (e.g. `Closes #12`).
