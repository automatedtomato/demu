Please follow this GitHub workflow for the MeshAnt repository.

> For commit message conventions, see [git-workflow.md](./git-workflow.md).
> For the pre-commit development process (planning, TDD, code review),
> see [development-workflow.md](./development-workflow.md).

## Branch model

- `main` = stable release branch
- `develop` = active development branch

Never do feature work directly on `main`.
Never start new implementation work from `main` unless explicitly instructed for release or hotfix purposes.

## Issue-driven development

All implementation work should be tied to a GitHub Issue.

### Basic rule

For each new task:

1. make sure there is a GitHub Issue
2. branch from `develop`
3. name the branch with the Issue number
4. implement the change on that branch
5. commit with clear messages
6. push the branch
7. open a Pull Request targeting `develop`
8. request review before merging
9. merge only after review

## Branch naming

For an Issue like `#12`, create a branch from `develop` using a name that includes the issue number.

Preferred format:

- `12-short-description`
- `issue-12-short-description`

Use a short, readable slug after the number.

Examples:

- `12-trace-draft-schema`
- `issue-18-interactive-cli-ingestion`

## Nested problems / follow-up issues

If a problem is discovered while working on an issue branch, do not silently fold unrelated work into the current branch.

Instead:

1. create a new GitHub Issue for the newly discovered problem
2. create a new branch from the current working branch if that is the most practical base
3. include the new Issue number in the new branch name
4. solve that problem in the new branch
5. open a PR
6. merge back appropriately
7. keep history visible and traceable

The goal is to make development flow visible on GitHub.

## Pull Requests

All code changes should be merged through Pull Requests.

### PR rules

- target branch should usually be `develop`
- PR title should clearly reference the Issue number
- PR description should explain:
  - what was changed
  - why it was changed
  - any tradeoffs
  - how it was tested
- keep PRs focused and reasonably small
- do not mix unrelated changes in one PR

## Review expectations

Before merging:

- review the PR for correctness
- check whether the change matches MeshAnt’s design documents
- check whether tests are present or updated when appropriate
- check whether documentation should also be updated
- check whether the branch is solving only the scoped issue

Do not merge directly without review unless explicitly instructed.

## Release flow

- `develop` is the integration branch for ongoing work
- `main` is only for stable release-ready states
- merge to `main` only when preparing or publishing a release

## Use of GitHub CLI

You may use `gh` commands for:

- creating issues
- viewing issues
- creating branches locally
- creating pull requests
- checking PR status
- reviewing PRs
- merging PRs

When working on a task, prefer making the workflow visible through GitHub objects rather than keeping context only in local git history.

## Operational expectations

When you start a new task:

1. check the current branch
2. check whether an Issue already exists
3. if needed, create the Issue
4. switch to `develop`
5. pull latest changes
6. create a new issue-linked branch
7. do the work there

When finishing a task:

1. run relevant checks/tests
2. commit clearly
3. push the branch
4. open a PR to `develop`
5. summarize the change clearly
6. wait for or perform review before merge

## Important constraint

Preserve a visible GitHub-based development history.

Do not hide substantial work inside large unreviewed local branches.
Do not accumulate unrelated edits before opening a PR.
Prefer explicit Issues, explicit branches, and explicit PRs.

## Default assumption

Unless told otherwise, assume:
- new work starts from `develop`
- work is tied to a GitHub Issue
- final merge target is `develop`
- `main` is reserved for stable releases
