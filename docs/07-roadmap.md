# Roadmap

## Versioning idea

Prefer small, demonstrable milestones.

## v0.1 — Dockerfile preview shell

Goal:
A user can preview a Dockerfile-derived world and inspect it interactively.

Includes:

- Rust CLI
- `demu -f Dockerfile`
- virtual filesystem
- `ls`, `cd`, `pwd`, `cat`, `find`, `env`
- support for `FROM`, `WORKDIR`, `COPY`, `ENV`
- `RUN` history recording
- initial `:layers`, `:history`

## v0.2 — useful RUN simulation

Goal:
Make preview state feel much more realistic.

Includes:

- filesystem mutation simulation
- package install registry
- `:installed`
- pseudo `which`, `pip list`, `apt list --installed`
- warnings for skipped commands

## v0.3 — provenance and multi-stage ✓ shipped

Goal:
Help users understand where things came from.

Includes:

- multi-stage support (`StageRegistry`, stage save/restore on `FROM`)
- `COPY --from=<stage>` (alias and numeric index, file and directory)
- `:explain <path>` (full provenance report)
- `--stage <name>` CLI flag, preserved across `:reload`

## v0.4 — Compose service preview

Goal:
Preview the world of a chosen service.

Includes:

- `--compose`
- `--service`
- `:services`
- `:mounts`
- env merge
- mount shadow explanation

## v0.5 — ergonomics

Possible additions:

- watch mode
- shell completion
- richer tree output
- compact TUI mode
- fixture browser

## Out-of-scope until much later

- exact base image filesystem extraction
- real container execution
- Kubernetes support
- full shell parser fidelity
