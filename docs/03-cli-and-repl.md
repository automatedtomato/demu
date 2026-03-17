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

## Supported shell commands for MVP

- `ls`
- `cd`
- `pwd`
- `cat`
- `find`
- `env`
- `exit`
- `help`

These operate on preview state, not the host shell.

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
Show visible mounts and shadowing behavior.

### `:services`
List Compose services in preview mode.

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
