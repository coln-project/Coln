# Coln Lab

One local workbench for Coln theories, stores, and synchronized graphs.

From the repository root:

```console
just examples/dev-web-lab
```

The command stages the browser compiler and starts or reuses Lab's local
Subduction relay. Build-time browser configuration can be placed in
`examples/lab/.env`; exported shell variables take precedence. See
`examples/lab/.env.example` for available settings.

The Store tool executes trusted JavaScript on the browser main thread. Do not
run programs from untrusted sources.

Lab owns its complete browser implementation, relay scripts, and graph schema.
Regenerate the checked-in graph bindings after changing `src/tools/graph/graph.coln`:

```console
pnpm --dir examples/lab graph:generate
```

Run unit and browser tests with `pnpm --dir examples/lab test` and
`pnpm --dir examples/lab test:e2e`.
