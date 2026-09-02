# Coln graph sync

This example edits one fixed Coln graph schema from a Svelte browser client or
an Ink terminal client. Both clients create or load the same Automerge document
and synchronize it through the bundled local Subduction relay.

The graph is directed and supports self-loops and parallel edges.

## Prerequisites

Build the local runtime and repo packages from the repository root:

```bash
npm ci --prefix packages/coln-js-runtime
npm run --prefix packages/coln-js-runtime build
pnpm --dir packages/coln-repo install
pnpm --dir packages/coln-repo build
pnpm --dir examples/sync install
```

## Browser

```bash
pnpm --dir examples/sync dev
```

Open the Vite URL. A URL without an Automerge document in its hash creates a
new document. Use the document URL shown in the inspector to open the same graph
in another browser or the CLI.

## CLI

Load an existing document:

```bash
pnpm --dir examples/sync cli <automerge-url>
```

Omit the URL to create a document in the CLI. Keep the CLI open while opening
its displayed URL in the browser because the bundled relay stores documents in
memory.

The CLI controls are:

- `v`: add vertex
- `e`: add edge using the selected source and target
- `Tab`: switch the active endpoint
- `Up`/`Down`: select a vertex for the active endpoint
- `/`: enter a TypeScript query callback body
- `q`: quit

## TypeScript queries

The query prompt accepts the body of a synchronous `handle.change` callback.
TypeScript syntax is transpiled but not semantically type-checked before it
runs; invalid transaction operations are reported as runtime errors.
For example:

```ts
tx.root.V.add()
```

Or using the lower-level transaction API:

```ts
tx.add("GraphRealm.V", [])
```

Queries execute as trusted local code and have the same access to the process as
other dynamically evaluated JavaScript. Do not run untrusted input.

Query compilation and execution live in `src/query/execute-query.ts`. That
module intentionally has no graph, Ink, Svelte, or synchronization imports. Its
public surface only depends on a target with a `change` method, allowing it to be
moved into a future CLI package and replaced here with a package import.

## Schema

The fixed schema is `graph.coln`. Regenerate its TypeScript bindings and JSON IR
with:

```bash
pnpm --dir examples/sync compile
```

## Verification

```bash
pnpm --dir examples/sync build
pnpm --dir examples/sync exec playwright install chromium # first time only
pnpm --dir examples/sync test:e2e
```

The end-to-end scenario exercises browser mutations, Node-side TypeScript query
execution, synchronization in both directions, and loading the resulting graph
in a second browser.
