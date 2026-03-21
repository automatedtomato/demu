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

## v0.4 — Compose service preview ✓ shipped

Goal:
Preview the world of a chosen service.

Shipped with:

- `--compose` and `--service` flags
- `:services`, `:mounts`, `:depends` commands
- Compose YAML parsing and service merge
- volume mount shadows and environment inheritance
- path traversal containment (security)
- `working_dir` root escape guard (security)
- `parse_env_file` comment and quote handling fixes
- `:depends` diamond dependency deduplication

## v0.5 — ergonomics

Planned additions:

- watch mode (file monitoring and re-simulation)
- shell completion (bash/zsh/fish)
- richer tree output (visual hierarchy for deep filesystems)
- compact TUI mode (alternative to line-based REPL)
- fixture browser (CLI for exploring included test fixtures)

## Out-of-scope until much later

- exact base image filesystem extraction
- real container execution
- Kubernetes support
- full shell parser fidelity
