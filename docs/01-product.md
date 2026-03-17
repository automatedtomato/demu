# Product definition

## One-sentence definition

`demu` is a fast preview shell that shows the expected world a Dockerfile or Compose service is trying to create.

## User problem

Many users do not struggle with app code first. They struggle with waiting, rebuilding, and guessing.

Typical pain:

- edit Dockerfile
- rebuild
- wait
- fail
- rebuild again
- open container just to run `ls -la`
- discover the mistake late

`demu` should reduce the cost of that loop.

## Target users

### Primary

- Developers debugging Compose service setup
- Solo developers doing personal projects
- Backend developers who want quick structure checks
- Junior engineers learning Docker
- Educators teaching Docker basics
- Teams reviewing Dockerfile changes

## Product promise

The user should be able to answer structural questions quickly:

- What files are present?
- Where am I in the filesystem?
- What got copied here?
- What packages appear installed?
- What came from which stage?
- What would this Compose service see?

## Core principles

### 1. Fast over exact

A useful preview in milliseconds is better than an exact model that takes too long.

### 2. Structural over behavioral

It is more important that files, env, stages, and mounts look right than that commands truly execute.

### 3. Safe over magical

No destructive operations on host state.
No hidden execution.
No silent side effects.

### 4. Explainability over completeness

When behavior is simulated, users should be able to understand the approximation.

## Non-goals

- Running production workloads
- Supporting every Dockerfile feature immediately
- Replacing Docker
- Modeling Kubernetes behavior
- Full shell compatibility

## MVP success criteria

The product is successful if a user can open a Dockerfile preview shell and quickly inspect:

- filesystem layout
- effective working directory
- environment variables
- simulated installed packages
- provenance of files
- history of interpreted instructions

## Product language

Use terms like:

- preview
- simulated
- expected world
- inspect
- explain

Avoid terms like:

- exact
- guaranteed identical
- fully emulated
- production-safe replacement
