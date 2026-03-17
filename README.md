# demu

`demu` is a fast preview shell for Dockerfile and Compose files.

It lets you inspect the **filesystem, env, stages, mounts, and simulated installed packages** a config is trying to create, **without fully building or running containers**.

## Why

When you are editing a `Dockerfile` or `compose.yaml`, you often just want to answer questions like:

- What files would be visible?
- What is the working directory?
- What env vars would exist?
- Did this `COPY` land where I think it did?
- Does this image look like it has `curl`, `git`, or `fastapi` installed?

`demu` is for that fast feedback loop.

## Example

```bash
demu -f Dockerfile
demu --compose -f compose.yaml --service api
```

Inside the preview shell:

```bash
ls -la
cd /app
cat package.json
find .
env
:layers
:history
:installed
:explain /app/main.py
exit
```

## What it is not

`demu` is **not**:

- a container runtime
- a Docker replacement
- a full shell emulator
- a dependency solver
- a Kubernetes tool

It prefers **fast, safe previews** over perfect fidelity.

## Status

This project is in active development.

The first target is a Dockerfile-focused MVP with:

- preview shell via `demu -f Dockerfile`
- virtual filesystem inspection
- support for `FROM`, `WORKDIR`, `COPY`, `ENV`
- partial `RUN` simulation for filesystem changes
- simulated install tracking for commands like `apt install` and `pip install`
- helper commands like `:layers`, `:history`, `:installed`, and `:explain`

Compose support comes after the Dockerfile MVP.

## Docs

- [Product](./docs/01-product.md)
- [Architecture](./docs/02-architecture.md)
- [CLI and REPL](./docs/03-cli-and-repl.md)
- [Dockerfile Semantics](./docs/04-dockerfile-semantics.md)
- [RUN Simulation](./docs/05-run-simulation.md)
- [Compose Plan](./docs/06-compose-plan.md)
- [Roadmap](./docs/07-roadmap.md)
- [Test Strategy](./docs/08-test-strategy.md)
