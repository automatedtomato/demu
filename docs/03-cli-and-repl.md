# CLI and REPL design

## UX goal

The product should feel like entering a lightweight preview shell.

The most important experience is:

```bash
demu -f Dockerfile
```

Then the user explores naturally.

## CLI surface

### Interactive mode

```bash
demu -f Dockerfile
demu -f Dockerfile --stage builder
demu --compose -f compose.yaml --service api
```

This starts a REPL.

### One-shot mode

```bash
demu exec -f Dockerfile -- ls /app
demu exec --compose -f compose.yaml --service api -- env
```

This is useful for scripts, editor integration, and tests.

## Prompt style

Keep it plain.

Example:

```text
demu:/{app}$ 
```

In Compose mode, optionally include service name:

```text
demu[api]:/{app}$ 
```

## Supported shell commands (Issue #5 — implemented, Compose support added in v0.4.0)

All commands operate on the preview state virtual filesystem and environment, not the host shell.

### Basic navigation and inspection

- `ls [path]` — list directory contents. Options: `-l`, `-la`, `-al` for long format
- `cd [path]` — change working directory; defaults to `/` when omitted
- `pwd` — print working directory
- `cat <path>` — print file contents (text files only)
- `find [path] [-name <pattern>]` — recursively search for files; path defaults to `/`; `-name` supports glob patterns

### Environment and session

- `env` — print all environment variables in sorted order
- `exit` / `quit` — exit the REPL session
- `help` — display command reference

### Implementation notes

- **Path resolution:** Both absolute (`/app/main.rs`) and relative (`../foo`) paths are resolved correctly.
- **Globbing:** The `find -name` command supports `*` and `?` glob patterns (simple glob, not full shell expansion).
- **Long format:** `ls -l` and `ls -la` display extended metadata when available (permissions, provenance source).
- **Error messages:** All errors are user-friendly and indicate whether a path is missing, a command is unsupported, or behavior is being simulated.

## Custom commands

Use a `:` prefix.

### `:layers`
Show interpreted instructions and state transitions by layer.

### `:history`
Show command history, including simulated and skipped `RUN` details.

### `:installed [manager]`
Show simulated installed packages.
Examples:

- `:installed`
- `:installed apt`
- `:installed pip`

### `:explain <path>`
Show provenance and metadata for a path.

### `:mounts`
Show visible mounts and shadowing behavior (Compose mode, v0.4.0).

### `:services`
List Compose services in preview mode (Compose mode, v0.4.0).

### `:depends`
Show the dependency tree for the current service, with diamond deduplication (Compose mode, v0.4.0).

### `:stage`
Show or change the current stage where supported.

## Pseudo-inspection commands to support later or early

These are not full implementations. They are convenience previews.

- `which <name>`
- `pip list`
- `apt list --installed`
- `npm list --depth=0`

These should read from the internal installed package registry.

## Error messaging

Good:

- `unsupported RUN command: make (recorded in history, not executed)`
- `path not found in preview filesystem: /app/bin`
- `command is available only in compose preview mode: :services`

Bad:

- `failed`
- `not implemented`
- silent no-op

## Watch mode

Not required for MVP, but design should not prevent a later watch mode.
