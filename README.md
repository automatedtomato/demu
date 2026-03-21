# demu

`demu` is a fast preview shell for Dockerfiles and Docker Compose files.

It lets you inspect the **filesystem, environment variables, and instruction history** a Dockerfile is trying to create — **without building or running a container**.

## Why

When you are editing a `Dockerfile` or `compose.yaml`, you often just want to answer questions like:

- What files would be visible?
- What is the working directory?
- What env vars would exist?
- Did this `COPY` land where I think it did?
- What did each instruction actually do?
- Where did this file come from — a `COPY`, a `RUN`, or a previous stage?
- What services are in this Compose file and what do they depend on?

`demu` is for that fast feedback loop.

## Install

### Pre-built binaries (recommended)

Download the latest release for your platform from the [Releases page](https://github.com/automatedtomato/demu/releases).

```bash
# Linux / macOS — extract and move to PATH
tar xzf demu-0.4.0-x86_64-unknown-linux-gnu.tar.gz
sudo mv demu-0.4.0-x86_64-unknown-linux-gnu/demu /usr/local/bin/
```

Available targets:

| Platform | File |
|----------|------|
| Linux x86_64 | `demu-*-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple Silicon / Intel via Rosetta 2) | `demu-*-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `demu-*-x86_64-pc-windows-msvc.zip` |

### From source

Requires Rust stable (`rustup` recommended).

```bash
git clone https://github.com/automatedtomato/demu.git
cd demu
cargo install --path .
```

## Usage

### Dockerfile mode

```bash
demu -f Dockerfile
```

This parses the Dockerfile, runs the simulation engine, prints any warnings, and drops you into an interactive preview shell.

For multi-stage Dockerfiles, use `--stage` to inspect a specific stage:

```bash
demu -f Dockerfile --stage builder
demu -f Dockerfile --stage 0   # by numeric index
```

### Compose mode

```bash
demu --compose compose.yaml --service api
```

This merges the selected service's configuration, simulates its Dockerfile, applies `working_dir`, `environment`, `env_file`, and volume mount shadows, then drops you into the same preview shell — now reflecting the service's effective state.

### Shell commands

| Command | Description |
|---------|-------------|
| `ls [path]` | List directory contents |
| `ls -la [path]` | List with details |
| `cd <path>` | Change directory |
| `pwd` | Print working directory |
| `cat <file>` | Print file contents |
| `find [path] [-name pattern]` | Search for files |
| `env` | Print all environment variables |
| `help` | Show available commands |
| `exit` | Quit the shell |

### Custom inspection commands

| Command | Description |
|---------|-------------|
| `:history` | Show each instruction and its effect, in order |
| `:layers` | Show a Docker-style layer summary |
| `:installed` | List all simulated package installs by manager |
| `:explain <path>` | Show where a file came from (provenance) |
| `:reload` | Re-read and re-simulate the Dockerfile in place |
| `:services` | List all services in the Compose file (Compose mode) |
| `:mounts` | Show volume mount shadows for the selected service (Compose mode) |
| `:depends` | Show the dependency tree for the selected service (Compose mode) |
| `which <cmd>` | Check whether a command appears to be installed |
| `apt list --installed` | apt-style installed package listing |
| `pip list` | pip-style installed package listing |

### Flags

| Flag | Description |
|------|-------------|
| `-f <path>` | Path to the Dockerfile (required in Dockerfile mode) |
| `--stage <name>` | Inspect a specific stage (name or numeric index) |
| `--compose <path>` | Path to a Compose file (enables Compose mode) |
| `--service <name>` | Service to inspect (required with `--compose`) |
| `--version` | Print version |
| `--help` | Print help |

### Demo

Try the included demo Dockerfile:

```bash
demu -f demo.dockerfile
```

Then explore:

```bash
pwd          # /app/src
cd /app
ls -la
cat config/app.conf
env
:history
:layers
exit
```

## What it is not

`demu` is **not**:

- a container runtime
- a Docker replacement
- a full shell emulator
- a dependency solver

It prefers **fast, safe previews** over perfect fidelity. Simulated behavior is always surfaced via warnings so you know what is approximated.

## Status

**v0.4.0** — Docker Compose service preview.

| Feature | Status |
|---------|--------|
| `FROM`, `WORKDIR`, `COPY`, `ENV` | Fully simulated |
| `COPY --from=<stage>` | Fully simulated |
| Multi-stage builds, `--stage` flag | Working |
| `RUN` filesystem commands (`mkdir`, `touch`, `rm`, `mv`, `cp`) | Simulated |
| `RUN` package installs (`apt-get`, `pip`, `npm`, `apk`) | Simulated |
| `ls`, `cd`, `pwd`, `cat`, `find`, `env` | Working |
| `:history`, `:layers` | Working |
| `:explain <path>` | Working |
| `:installed`, `which`, `:reload` | Working |
| `apt list --installed`, `pip list` | Working |
| Skipped-command warnings with reason | Working |
| Compose YAML parsing | Working |
| Compose service preview (`--compose`, `--service`) | Working |
| Compose `:services`, `:mounts`, `:depends` | Working |
| Volume mount shadows | Working |
| Path traversal containment (build.context/dockerfile) | Working |

See the [roadmap](./docs/07-roadmap.md) for the full plan.

## Building and testing

```bash
cargo build
cargo test
```

## Dev environment

A containerized dev environment is available via Docker Compose:

```bash
docker compose -f docker-compose.dev.yml run --rm dev bash
```

## Docs

- [Product](./docs/01-product.md)
- [Architecture](./docs/02-architecture.md)
- [CLI and REPL](./docs/03-cli-and-repl.md)
- [Dockerfile Semantics](./docs/04-dockerfile-semantics.md)
- [RUN Simulation](./docs/05-run-simulation.md)
- [Compose Plan](./docs/06-compose-plan.md)
- [Roadmap](./docs/07-roadmap.md)
- [Test Strategy](./docs/08-test-strategy.md)
