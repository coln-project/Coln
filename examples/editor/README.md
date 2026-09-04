# Coln Store Lab

A Svelte 5 workbench for inspecting and programming synchronized Coln stores.

```console
pnpm install
pnpm dev
```

The development command uses the shared managed relay in `examples/sync-server`.
Set `SUBDUCTION_PORT` to choose another local relay port. Production builds use
`VITE_SUBDUCTION_ENDPOINT`, falling back to the public Subduction relay.

The JavaScript REPL runs trusted code on the main browser thread with the loaded
Coln `handle` in scope. Its outer program may be asynchronous, but callbacks
passed to `handle.change` must remain synchronous.
