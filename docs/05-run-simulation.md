# RUN simulation policy

## Goal

The job of `RUN` support is not to execute commands faithfully.
The job is to produce a useful preview of what a user expects to see afterward.

That means:

- simulate filesystem changes when feasible
- record package installs in a registry
- surface unsupported parts clearly

## General policy

For MVP, split `RUN` command chains on simple separators:

- `&&`
- `;`

Interpret each segment independently with a safe subset.

If a segment is unsupported:

- keep it in history
- mark it as skipped/unmodeled
- continue if policy allows

## Supported filesystem mutations

These are high-value and should be supported early:

- `mkdir`
- `touch`
- `cp`
- `mv`
- `rm`
- `ln -s`
- `chmod` (metadata only is acceptable)
- `chown` (metadata only is acceptable)
- `echo foo > file`
- `echo foo >> file`
- `cd` (within the current RUN chain only)
- `pwd` (diagnostic only)

## Supported install simulations

These should not download real packages.
They should only record package names under an install manager.

### Apt

Support recognizing forms like:

- `apt install -y curl git`
- `apt-get install -y curl git`

### Pip

Support recognizing forms like:

- `pip install fastapi uvicorn`
- `python -m pip install fastapi`

### Mim

Support recognizing forms like:

- `mim install mmcv`

### Go

Support recognizing forms like:

- `go get github.com/foo/bar`
- `go install github.com/foo/bar@latest`

### Npm / pnpm / yarn

Later but likely useful:

- `npm install express`
- `pnpm add react`
- `yarn add axios`

## Internal registry model

Suggested structure:

```rust
struct InstalledRegistry {
    apt: BTreeSet<String>,
    pip: BTreeSet<String>,
    mim: BTreeSet<String>,
    go: BTreeSet<String>,
    npm: BTreeSet<String>,
}
```

## Pseudo-inspection commands

These commands should read from `InstalledRegistry` and return plausible output:

- `:installed`
- `:installed apt`
- `which curl`
- `pip list`
- `apt list --installed`

## Transparency requirement

Never imply that installs were real.
Good output should make simulation obvious.

Examples:

- `curl (simulated apt install)`
- `fastapi (simulated pip install)`

## Unsupported commands

Examples of commands that should not run in MVP:

- `make`
- `cmake`
- `cargo build`
- `python setup.py install`
- arbitrary shell scripts
- `curl https://... | sh`

These should be recorded and surfaced, not executed.

## Example

Input:

```dockerfile
RUN mkdir -p /app/logs && touch /app/logs/app.log && apt-get install -y curl git && pip install fastapi uvicorn
```

Expected preview effect:

- `/app/logs/` exists
- `/app/logs/app.log` exists
- apt registry contains `curl`, `git`
- pip registry contains `fastapi`, `uvicorn`
- history clearly marks all install actions as simulated
