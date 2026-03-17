# Compose support plan

## Why Compose matters

Many users do not struggle with a single image.
They struggle with app + db + redis + env + mounts.

Compose support is therefore valuable, but should come after Dockerfile MVP.

## Compose for demu is a preview, not orchestration

`demu` should not start containers.
It should present the expected view of a selected service.

## MVP for Compose support

Support only what is needed to inspect a service preview:

- `services`
- `build`
- `image`
- `environment`
- `volumes`
- `working_dir`
- `depends_on`
- `ports` as metadata only

## Primary experience

```bash
demu --compose -f compose.yaml --service api
```

This should:

1. parse compose file
2. select service `api`
3. resolve its Dockerfile or image preview if supported
4. merge service environment
5. apply mount metadata
6. open REPL

## Custom commands for Compose mode

- `:services`
- `:mounts`
- `:service`
- `:depends`

## Mount behavior

A core value of Compose mode is showing what a mount would shadow.

Example:

- image preview contains `/app/node_modules`
- bind mount overlays `/app`

The preview should be able to explain that some paths are shadowed by mounts.

## Compose phase boundaries

### Phase 1

- list services
- choose one service
- show merged env and mount metadata
- open preview shell from build-linked Dockerfile if available

### Phase 2

- shadowing-aware path explanation
- simple `depends_on` metadata views
- image-only service preview support if an image source is later added

### Phase 3

- better `.env` and variable interpolation
- profile support if needed

## Important constraint

Compose support must not distort the product into a container orchestrator.
Keep it a service preview layer.
